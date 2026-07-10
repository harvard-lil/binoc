use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::{
    Annotation, ArtifactFormat, BinocError, BinocResult, DataAccess, Diagnostic, ExtractResult,
    GlobalClaim, IdentityExtractor, IdentityFailurePolicy, IdentityToken, ItemRef, NodeIdentity,
    RowIdentity, Segment, Summary,
};

/// Which side tree a node belongs to in the correspondence-first IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TreeSide {
    Left,
    Right,
}

impl TreeSide {
    pub fn label(self) -> &'static str {
        match self {
            TreeSide::Left => "left",
            TreeSide::Right => "right",
        }
    }
}

/// Stable identity of one side-tree node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeId {
    pub side: TreeSide,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ArtifactDecodeCacheKey {
    id: NodeId,
    format: ArtifactFormat,
    type_id: TypeId,
}

#[derive(Clone)]
enum CachedDecode {
    Missing,
    Present(Arc<dyn Any + Send + Sync>),
}

/// Per-run, type-erased cache for decoded artifacts used by in-process rules.
///
/// The engine only carries this cache through dispatch; individual plugins own
/// the artifact format and decoded type they store in it.
#[derive(Default)]
pub struct ArtifactDecodeCache {
    entries: Mutex<HashMap<ArtifactDecodeCacheKey, CachedDecode>>,
}

impl ArtifactDecodeCache {
    pub fn get_or_try_insert_with<T>(
        &self,
        id: NodeId,
        format: &ArtifactFormat,
        load: impl FnOnce() -> BinocResult<Option<T>>,
    ) -> BinocResult<Option<Arc<T>>>
    where
        T: Any + Send + Sync,
    {
        let key = ArtifactDecodeCacheKey {
            id,
            format: format.clone(),
            type_id: TypeId::of::<T>(),
        };
        if let Some(cached) = self.lookup::<T>(&key)? {
            return Ok(cached);
        }

        let loaded = match load()? {
            Some(value) => CachedDecode::Present(Arc::new(value)),
            None => CachedDecode::Missing,
        };

        let cached = {
            let mut entries = self.entries.lock().map_err(cache_poisoned)?;
            entries.entry(key).or_insert(loaded).clone()
        };
        decode_cached::<T>(cached)
    }

    fn lookup<T>(&self, key: &ArtifactDecodeCacheKey) -> BinocResult<Option<Option<Arc<T>>>>
    where
        T: Any + Send + Sync,
    {
        let cached = self
            .entries
            .lock()
            .map_err(cache_poisoned)?
            .get(key)
            .cloned();
        cached.map(decode_cached::<T>).transpose()
    }
}

fn decode_cached<T>(cached: CachedDecode) -> BinocResult<Option<Arc<T>>>
where
    T: Any + Send + Sync,
{
    match cached {
        CachedDecode::Missing => Ok(None),
        CachedDecode::Present(value) => value
            .downcast::<T>()
            .map(Some)
            .map_err(|_| BinocError::Other("artifact decode cache type mismatch".into())),
    }
}

fn cache_poisoned<T>(_err: std::sync::PoisonError<T>) -> BinocError {
    BinocError::Other("artifact decode cache lock poisoned".into())
}

/// Product-facing projection metadata supplied by rules, not inferred by core.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectionHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Tags this hint *removes* from the accumulated projection. Tag overlay is
    /// union-only, so an annotator that supersedes an earlier framing (e.g. a
    /// CFM-71 container reshape replacing a pair-time `binoc.move`) needs a way to
    /// drop the now-stale tag — otherwise the IR carries contradictory tags
    /// (inert in rendering, but incoherent in JSON). A retraction is honored
    /// whenever tags are merged: the named tags are removed from the result and
    /// can never be re-introduced by the *same* hint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retract_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
}

pub fn projection_hint_is_default(hint: &ProjectionHint) -> bool {
    hint == &ProjectionHint::default()
}

impl ProjectionHint {
    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn item_type(mut self, item_type: impl Into<String>) -> Self {
        self.item_type = Some(item_type.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Declare that this hint retracts `tag` from the accumulated projection —
    /// used to drop a superseded framing (e.g. a reshape annotator dropping the
    /// pair-time `binoc.move`). See [`ProjectionHint::retract_tags`].
    pub fn retract_tag(mut self, tag: impl Into<String>) -> Self {
        self.retract_tags.push(tag.into());
        self
    }

    pub fn summary(mut self, summary: impl Into<Summary>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn annotate(
        mut self,
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        upsert_annotation(&mut self.annotations, package.into(), key.into(), value);
        self
    }

    pub fn merge_from(&mut self, other: &ProjectionHint) {
        if self.action.is_none() {
            self.action = other.action.clone();
        }
        if self.item_type.is_none() {
            self.item_type = other.item_type.clone();
        }
        if self.summary.is_none() {
            self.summary = other.summary.clone();
        }
        self.merge_tags(other);
        merge_annotations_if_missing(&mut self.annotations, &other.annotations);
    }

    /// Union `other`'s tags and retractions into `self`, then honor the combined
    /// retraction set so the result never carries a retracted tag. Shared by
    /// `merge_from` and `overlay_from` — the single point where tag sets combine.
    fn merge_tags(&mut self, other: &ProjectionHint) {
        self.tags.extend(other.tags.iter().cloned());
        self.tags.sort();
        self.tags.dedup();
        self.retract_tags.extend(other.retract_tags.iter().cloned());
        self.retract_tags.sort();
        self.retract_tags.dedup();
        if !self.retract_tags.is_empty() {
            self.tags.retain(|tag| !self.retract_tags.contains(tag));
        }
    }

    /// Overlay `other` onto `self`: every field `other` sets wins (unlike
    /// [`merge_from`](Self::merge_from), which only fills gaps). Tags union.
    pub fn overlay_from(&mut self, other: &ProjectionHint) {
        if other.action.is_some() {
            self.action = other.action.clone();
        }
        if other.item_type.is_some() {
            self.item_type = other.item_type.clone();
        }
        if other.summary.is_some() {
            self.summary = other.summary.clone();
        }
        self.merge_tags(other);
        overlay_annotations(&mut self.annotations, &other.annotations);
    }
}

fn merge_annotations_if_missing(target: &mut Vec<Annotation>, source: &[Annotation]) {
    for annotation in source {
        if !target.iter().any(|existing| {
            existing.package == annotation.package && existing.key == annotation.key
        }) {
            target.push(annotation.clone());
        }
    }
}

fn overlay_annotations(target: &mut Vec<Annotation>, source: &[Annotation]) {
    for annotation in source {
        upsert_annotation(
            target,
            annotation.package.clone(),
            annotation.key.clone(),
            annotation.value.clone(),
        );
    }
}

fn upsert_annotation(
    annotations: &mut Vec<Annotation>,
    package: String,
    key: String,
    value: serde_json::Value,
) {
    if let Some(existing) = annotations
        .iter_mut()
        .find(|annotation| annotation.package == package && annotation.key == key)
    {
        existing.value = value;
    } else {
        annotations.push(Annotation::new(package, key, value));
    }
}

pub struct ProjectionAnnotationContext<'a> {
    pub action: &'a str,
    pub item_type: &'a str,
    pub path: &'a str,
    pub source_path: Option<&'a str>,
    /// `item_type` of the *source* (left/from) endpoint of a link, when this line
    /// is a reconciled correspondence. Lets an annotator notice that a container's
    /// representation changed (e.g. "directory" -> "SQLite database") and render a
    /// reshape instead of a bare move. `None` for unlinked add/remove lines and
    /// when the source carried no explicit item_type. Core supplies the raw
    /// strings; it never interprets them — the annotator owns the wording.
    pub source_item_type: Option<&'a str>,
    pub evidence: Option<&'a str>,
    pub edits: &'a [Edit],
    pub container: bool,
    pub unlinked_side: Option<TreeSide>,
}

pub trait ProjectionAnnotator: Send + Sync {
    fn name(&self) -> &str;
    fn annotate(&self, ctx: &ProjectionAnnotationContext<'_>) -> ProjectionHint;
}

/// One rule registered with the correspondence-first saturation engine.
#[derive(Clone)]
pub enum CoreRule {
    Expand(Arc<dyn ExpandRule>),
    Parse(Arc<dyn ParseRule>),
    Pair(Arc<dyn PairRule>),
}

impl CoreRule {
    pub fn name(&self) -> String {
        match self {
            CoreRule::Expand(rule) => rule.descriptor().name,
            CoreRule::Parse(rule) => rule.descriptor().name,
            CoreRule::Pair(rule) => rule.descriptor().name,
        }
    }
}

/// In-process registration surface for correspondence rule packs.
///
/// The engine that consumes this type lives in `binoc-core`, but the type stays
/// in the SDK so stdlib and third-party packs can be configured without
/// depending on host internals.
#[derive(Default, Clone)]
pub struct CorrespondenceEngineConfig {
    pub rules: Vec<CoreRule>,
    pub writers: Vec<Arc<dyn EditListWriter>>,
    pub compaction: Vec<Arc<dyn CompactionRule>>,
    pub annotators: Vec<Arc<dyn ProjectionAnnotator>>,
    /// Partition-identity extractors, keyed by artifact format (CFM-72). The
    /// engine dispatches these JIT over the *unmatched* residue when a
    /// partition-capable pair rule asks for a node's identity tokens; they are
    /// never stored in the IR or gold. A format with no extractor here is simply
    /// not partition-capable.
    pub identity_extractors: Vec<Arc<dyn IdentityExtractor>>,
    pub row_keys: BTreeMap<String, Vec<String>>,
    pub row_identity_policies: BTreeMap<String, RowIdentityPolicies>,
    pub node_identities: BTreeMap<String, NodeIdentity>,
    pub root_projection: ProjectionHint,
    pub dataset_configurator: Option<Arc<dyn CorrespondenceDatasetConfigurator>>,
    /// Optional path-scoped dispatch resolver installed by a rule pack's dataset
    /// configurator. Core treats it as opaque: it may annotate an item before
    /// declarative dispatch and may restrict dispatch to a named rule for that
    /// item.
    pub dispatch_resolver: Option<Arc<dyn DispatchResolver>>,
}

pub trait CorrespondenceDatasetConfigurator: Send + Sync {
    fn configure(
        &self,
        config: &mut CorrespondenceEngineConfig,
        dataset: &serde_json::Value,
        left_root: &ItemRef,
        right_root: &ItemRef,
        data: &dyn DataAccess,
    ) -> BinocResult<Vec<Diagnostic>>;
}

pub trait DispatchResolver: Send + Sync {
    fn configure_item(&self, item: &mut ItemRef) -> BinocResult<Vec<Diagnostic>>;

    fn forced_rule_for(&self, _item: &ItemRef) -> Option<String> {
        None
    }

    fn row_identity_for(&self, _path: &str) -> Option<RowIdentity> {
        None
    }

    fn node_identity_for(&self, _path: &str) -> Option<NodeIdentity> {
        None
    }
}

/// Metadata-only declarative filter over an [`ItemRef`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_dir: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_types: Vec<String>,
}

impl NodeMatch {
    pub fn matches(&self, item: &ItemRef) -> bool {
        if let Some(expected) = self.is_dir {
            if item.is_dir != expected {
                return false;
            }
        }
        if !self.extensions.is_empty() {
            let ext = item.extension();
            if !ext
                .as_ref()
                .is_some_and(|ext| self.extensions.iter().any(|candidate| candidate == ext))
            {
                return false;
            }
        }
        if !self.media_types.is_empty() {
            let media_type = item.media_type.as_deref().unwrap_or("");
            if !self
                .media_types
                .iter()
                .any(|candidate| candidate == media_type)
            {
                return false;
            }
        }
        true
    }
}

/// Shape filter for edit-list writers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ShapeFilter {
    #[default]
    Any,
    Container,
    Leaf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExpandDescriptor {
    pub name: String,
    pub input: NodeMatch,
    #[serde(default)]
    pub fires_beneath_settled: bool,
}

pub trait ExpandRule: Send + Sync {
    fn descriptor(&self) -> ExpandDescriptor;
    fn expand(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ExpandOutput>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExpandOutput {
    pub children: Vec<ItemRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl From<Vec<ItemRef>> for ExpandOutput {
    fn from(children: Vec<ItemRef>) -> Self {
        Self {
            children,
            diagnostics: Vec::new(),
        }
    }
}

/// One slot in a parse rule's correlated input member-set (CFM-83).
///
/// A member-match is a [`NodeMatch`] plus whether the slot must be filled for a
/// group to form. The ordered member list of a [`ParseDescriptor`] is its
/// `input` anchor (always a required size-1 member) followed by any
/// `extra_members`. A single-input parser declares no extra members, so its
/// member-set is exactly `[{ input, required: true }]` — the size-1 degenerate
/// case the engine still drives through the same enumeration path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MemberMatch {
    #[serde(rename = "match")]
    pub matcher: NodeMatch,
    #[serde(default)]
    pub required: bool,
}

impl MemberMatch {
    /// A required member slot.
    pub fn required(matcher: NodeMatch) -> Self {
        Self {
            matcher,
            required: true,
        }
    }

    /// An optional member slot — a group may form without it.
    pub fn optional(matcher: NodeMatch) -> Self {
        Self {
            matcher,
            required: false,
        }
    }
}

/// A plain `NodeMatch` is the size-1 required member: the ergonomic single-input
/// case promised by CFM-83's ADR.
impl From<NodeMatch> for MemberMatch {
    fn from(matcher: NodeMatch) -> Self {
        MemberMatch::required(matcher)
    }
}

/// How the engine groups candidate sibling nodes into one parse-claim input.
///
/// `SharedStem` (the default) groups a container's children by *shared basename
/// under the same parent*, where the basename is the file name with only its
/// final extension removed (`roads.v2.shp` and `roads.v2.dbf` share `roads.v2`;
/// `roads.shp` stays `roads`). This is the only generic, format-agnostic grouping
/// knowledge core needs, and it keeps versioned sibling sets distinct. The
/// capture/template generalization (for suffix sidecars named *off* an anchor
/// stem rather than sharing it, e.g. `data.tif` + `data.tif.aux.xml`) is a
/// deferred seam; it reuses `DeclaredPair`'s `selector_captures`/`expand_template`
/// vocabulary. Until a real format needs it, only `SharedStem` is implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Correlation {
    /// Same parent container + shared basename stem.
    #[default]
    SharedStem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParseDescriptor {
    pub name: String,
    /// The anchor member: the required, defining node of the claim (e.g. the
    /// `.shp`). This stays a plain [`NodeMatch`] so the 1-input case — the
    /// overwhelming majority of parse rules — is unchanged. Additional members
    /// and the correlation key for a fusing (multi-input) claim are declared on
    /// the [`ParseRule`] trait ([`ParseRule::extra_members`] /
    /// [`ParseRule::correlation`]), keeping the blast radius of CFM-83 off every
    /// single-input descriptor literal.
    pub input: NodeMatch,
    pub output: ArtifactFormat,
    #[serde(default)]
    pub fires_beneath_settled: bool,
}

/// A resolved group of member nodes handed to a multi-input [`ParseRule`].
///
/// The `anchor` is the required defining node (always present). `members` holds
/// the resolved [`ItemRef`] for every slot in descriptor order, `None` for an
/// unfilled optional slot. Index 0 is always the anchor (`Some`). A single-input
/// parse sees a group with just the anchor.
#[derive(Debug, Clone)]
pub struct ParseGroup {
    pub anchor: ItemRef,
    pub members: Vec<Option<ItemRef>>,
}

impl ParseGroup {
    /// A trivial size-1 group wrapping a single anchor node.
    pub fn single(anchor: ItemRef) -> Self {
        Self {
            members: vec![Some(anchor.clone())],
            anchor,
        }
    }

    /// The resolved member at slot `index` (descriptor order), if filled.
    pub fn member(&self, index: usize) -> Option<&ItemRef> {
        self.members.get(index).and_then(Option::as_ref)
    }

    /// All filled members (anchor + present optionals), in slot order.
    pub fn present(&self) -> impl Iterator<Item = &ItemRef> {
        self.members.iter().filter_map(Option::as_ref)
    }
}

pub trait ParseRule: Send + Sync {
    fn descriptor(&self) -> ParseDescriptor;

    /// Parse a single anchor node. This is the single-input entry point every
    /// ordinary parser implements; the member-set generalization (CFM-83) does
    /// not touch it.
    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput>;

    /// Additional member slots beyond the anchor (`descriptor().input`), in
    /// order — e.g. `.shx`, `.dbf`, `.prj`, `.cpg` for a fusing shapefile claim.
    /// The default is empty: a single-input claim. The full ordered member-set
    /// is the anchor (always a required size-1 member) followed by these; see
    /// [`member_set`].
    fn extra_members(&self) -> Vec<MemberMatch> {
        Vec::new()
    }

    /// How candidate sibling groups are enumerated for a multi-input claim.
    /// Ignored when [`extra_members`](Self::extra_members) is empty.
    fn correlation(&self) -> Correlation {
        Correlation::SharedStem
    }

    /// Parse a resolved correlated member group. The default delegates to
    /// [`parse`](Self::parse) on the anchor, so single-input rules need not
    /// implement it. A fusing rule (e.g. the shapefile layer) overrides this to
    /// read its `.shp`/`.dbf`/`.prj` members together and emit one fused node;
    /// it returns an empty [`ParseOutput`] to **decline** when the group is not a
    /// real instance of its format, releasing the members to smaller claims.
    fn parse_group(&self, group: &ParseGroup, data: &dyn DataAccess) -> BinocResult<ParseOutput> {
        self.parse(&group.anchor, data)
    }
}

/// The full ordered member-set of a parse rule: the anchor (always a required
/// size-1 member) followed by the rule's [`extra_members`](ParseRule::extra_members).
/// This is the list the engine fills by [`NodeMatch`] when enumerating candidate
/// sibling groups; index 0 is always the required anchor.
pub fn member_set(rule: &dyn ParseRule) -> Vec<MemberMatch> {
    let mut members = vec![MemberMatch::required(rule.descriptor().input)];
    members.extend(rule.extra_members());
    members
}

/// A parse claim's arity: the number of declared member slots (anchor + extras).
/// Drives arity-descending precedence — larger claims are attempted first.
pub fn parse_arity(rule: &dyn ParseRule) -> usize {
    1 + rule.extra_members().len()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParseOutput {
    pub bytes: Vec<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ParsedChild>,
    /// Additional artifacts to publish on the parsed node itself, beyond the
    /// primary `bytes` artifact (whose format is the descriptor's `output`).
    /// This is the channel for a second artifact on a node — e.g. a
    /// `parser_metadata_v1` bag riding alongside a `tabular_v1` leaf, or on a
    /// container that publishes no primary `bytes`. Each rides as its own
    /// format, diffed independently by a format-matched writer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ParsedArtifact>,
    /// Projection overlay for the node being parsed. A container parse (one that
    /// emits children and no parent artifact) uses this to name what kind of
    /// container the node is — e.g. `item_type("SQLite database")` — since the
    /// node would otherwise inherit only an extension-based guess. Fields set
    /// here win over the node's existing projection (see
    /// [`ProjectionHint::overlay_from`]).
    #[serde(default, skip_serializing_if = "projection_hint_is_default")]
    pub projection: ProjectionHint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParsedChild {
    pub item: ItemRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ParsedArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParsedArtifact {
    pub format: ArtifactFormat,
    pub bytes: Vec<u8>,
}

impl From<Vec<u8>> for ParseOutput {
    fn from(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            diagnostics: Vec::new(),
            children: Vec::new(),
            artifacts: Vec::new(),
            projection: ProjectionHint::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PairDescriptor {
    pub name: String,
    #[serde(default)]
    pub emits: Vec<String>,
    /// Artifact formats this rule consumes pre-link to decide pairings.
    ///
    /// This is a declared read-set, the pairing-side analogue of a parse
    /// rule's `output`. A rule that pairs nodes by their parsed content (rather
    /// than by raw bytes, hashes, or paths) lists those formats here so the
    /// engine knows the artifacts must be materialized on unlinked nodes before
    /// the rule runs. Rules that read no artifacts leave this empty.
    #[serde(default)]
    pub reads: Vec<ArtifactFormat>,
    #[serde(default)]
    pub sees_beneath_settled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LinkProposal {
    pub left: u32,
    pub right: u32,
    pub evidence: String,
    #[serde(default)]
    pub settled: bool,
    #[serde(default)]
    pub projection: ProjectionHint,
}

pub trait PairRule: Send + Sync {
    fn descriptor(&self) -> PairDescriptor;
    fn propose(&self, view: &dyn EngineView, data: &dyn DataAccess) -> BinocResult<PairOutput>;
    fn final_diagnostics(
        &self,
        _view: &dyn EngineView,
        _data: &dyn DataAccess,
    ) -> BinocResult<Vec<Diagnostic>> {
        Ok(Vec::new())
    }

    /// Global, non-tree claims this rule asserts about the *final* settled link
    /// graph (CFM-72). Called once after saturation, like
    /// [`final_diagnostics`](Self::final_diagnostics); the engine collects the
    /// result into `Changeset.claims`. A rule that reshapes the link set into a
    /// split/merge fan-out reports the claim here so the assertion is produced
    /// once, from the converged state, rather than re-emitted every round.
    fn final_claims(
        &self,
        _view: &dyn EngineView,
        _data: &dyn DataAccess,
    ) -> BinocResult<Vec<GlobalClaim>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PairOutput {
    pub proposals: Vec<LinkProposal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl From<Vec<LinkProposal>> for PairOutput {
    fn from(proposals: Vec<LinkProposal>) -> Self {
        Self {
            proposals,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LinkRef {
    pub index: usize,
    pub left: NodeId,
    pub right: NodeId,
    pub evidence: String,
    pub proposer: String,
    pub priority: u32,
    pub settled: bool,
    #[serde(default)]
    pub projection: ProjectionHint,
}

pub trait EngineView {
    fn root(&self, side: TreeSide) -> NodeId;
    fn visible(&self, id: NodeId) -> bool;
    fn nodes(&self, side: TreeSide) -> Vec<NodeId>;
    fn item(&self, id: NodeId) -> &ItemRef;
    fn parent(&self, id: NodeId) -> Option<NodeId>;
    fn children(&self, id: NodeId) -> Vec<NodeId>;
    fn has_children(&self, id: NodeId) -> bool;
    fn is_linked(&self, id: NodeId) -> bool;
    fn links(&self) -> Vec<LinkRef>;
    fn links_of(&self, id: NodeId) -> Vec<LinkRef>;
    fn artifact_bytes(
        &self,
        id: NodeId,
        format: &ArtifactFormat,
        data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<u8>>>;

    /// Partition-identity tokens for a node (CFM-72), or `None` when no
    /// registered [`IdentityExtractor`] matches an artifact the node carries.
    ///
    /// The engine owns the dispatch: it tries each registered extractor's format
    /// against the node's artifacts and runs the first match. The rule stays
    /// format-ignorant — it sees only opaque, globally-comparable tokens — so the
    /// same partition rule serves every partition-capable format. Computed JIT
    /// over whatever node the caller asks about (intended: the unmatched
    /// residue); never stored.
    fn identity_tokens(
        &self,
        _id: NodeId,
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<IdentityToken>>> {
        Ok(None)
    }
}

/// One open-vocabulary edit in a link's edit list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Edit {
    pub verb: String,
    pub params: serde_json::Value,
    #[serde(default)]
    pub projection: EditProjection,
    /// Provenance tag: which content type produced this edit. For an artifact
    /// writer it is the artifact format's display string (e.g.
    /// `binoc.tabular.v1`); for a structural writer (container/text/fallback) it
    /// is the writer's name. Set by the dispatcher after a writer runs — writers
    /// do not populate it themselves — so the merged per-link edit list can be
    /// sliced back into per-content-type segments for format-scoped compaction,
    /// extract routing, and grouped summary/projection. `None` only for
    /// hand-built edits in tests that never pass through dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

impl Edit {
    pub fn new(verb: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            verb: verb.into(),
            params,
            projection: EditProjection::default(),
            provenance: None,
        }
    }

    /// Stamp this edit's provenance (the producing format/writer). Used by the
    /// dispatcher; idempotent and chainable.
    pub fn with_provenance(mut self, provenance: impl Into<String>) -> Self {
        self.provenance = Some(provenance.into());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.projection.visible = false;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.projection.hint.tags.push(tag.into());
        self
    }

    pub fn with_item_type(mut self, item_type: impl Into<String>) -> Self {
        self.projection.hint.item_type = Some(item_type.into());
        self
    }

    pub fn with_summary(mut self, summary: impl Into<Summary>) -> Self {
        self.projection.hint.summary = Some(summary.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EditProjection {
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default)]
    pub hint: ProjectionHint,
}

impl Default for EditProjection {
    fn default() -> Self {
        Self {
            visible: true,
            hint: ProjectionHint::default(),
        }
    }
}

fn default_visible() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WriterDescriptor {
    pub name: String,
    #[serde(default)]
    pub formats: Vec<ArtifactFormat>,
    pub input: NodeMatch,
    #[serde(default)]
    pub shape: ShapeFilter,
    /// Marks the last-resort structural writer (the byte/hash fallback). Under
    /// composing dispatch (CFM-81) the fallback runs only when no other writer
    /// claimed the link; a fallback writer always declares empty `formats`.
    #[serde(default)]
    pub fallback: bool,
}

pub struct LinkCtx<'a> {
    pub view: &'a dyn EngineView,
    pub link: LinkRef,
    pub row_keys: Cow<'a, [String]>,
    pub row_identity_policies: RowIdentityPolicies,
    pub node_identity: Option<Cow<'a, NodeIdentity>>,
    pub artifact_cache: &'a ArtifactDecodeCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowIdentityPolicies {
    pub on_null_key: IdentityFailurePolicy,
    pub on_duplicate_key: IdentityFailurePolicy,
}

impl Default for RowIdentityPolicies {
    fn default() -> Self {
        Self {
            on_null_key: IdentityFailurePolicy::Diagnostic,
            on_duplicate_key: IdentityFailurePolicy::Diagnostic,
        }
    }
}

pub trait EditListWriter: Send + Sync {
    fn descriptor(&self) -> WriterDescriptor;
    fn write(&self, ctx: &LinkCtx<'_>, data: &dyn DataAccess) -> BinocResult<Option<WriteOutput>>;
    fn extract(
        &self,
        _ctx: &LinkCtx<'_>,
        _edits: &[Edit],
        _aspect: &str,
        _data: &dyn DataAccess,
    ) -> BinocResult<Option<ExtractResult>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WriteOutput {
    pub edits: Vec<Edit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl From<Vec<Edit>> for WriteOutput {
    fn from(edits: Vec<Edit>) -> Self {
        Self {
            edits,
            diagnostics: Vec::new(),
        }
    }
}

pub trait CompactionRule: Send + Sync {
    fn name(&self) -> &str;

    /// The artifact format whose provenance-scoped segment this rule rewrites.
    /// The dispatcher slices a link's merged edit list down to the edits tagged
    /// with this format before calling [`rewrite`](Self::rewrite), so a rule
    /// never sees or rewrites another content type's edits. `None` means the
    /// rule operates on the whole (unsegmented) edit list — reserved for
    /// cross-content-type or structural compaction; format-specific rules must
    /// declare their format.
    fn format(&self) -> Option<ArtifactFormat> {
        None
    }

    fn rewrite(
        &self,
        ctx: &LinkCtx<'_>,
        edits: &[Edit],
        data: &dyn DataAccess,
    ) -> BinocResult<Option<Vec<Edit>>>;
}

/// Generic summary for edit-count fallback projection.
pub fn edit_count_summary(edit_count: usize) -> Summary {
    Summary(vec![
        Segment::Uint(edit_count as u64),
        Segment::Text(format!(" edit{}", if edit_count == 1 { "" } else { "s" })),
    ])
}

#[cfg(test)]
mod projection_hint_tests {
    use super::*;

    #[test]
    fn overlay_retracts_a_superseded_tag() {
        // A reshape framing supersedes a pair-time move: the move tag must not
        // survive into the accumulated projection, even though overlay is
        // otherwise union-only.
        let mut acc = ProjectionHint::default()
            .tag("binoc.move")
            .tag("binoc.keep");
        let reshape = ProjectionHint::default()
            .tag("binoc.container-reshape")
            .retract_tag("binoc.move");
        acc.overlay_from(&reshape);
        assert!(acc.tags.contains(&"binoc.container-reshape".to_string()));
        assert!(acc.tags.contains(&"binoc.keep".to_string()));
        assert!(!acc.tags.contains(&"binoc.move".to_string()));
    }

    #[test]
    fn retraction_holds_regardless_of_union_order() {
        // Retracting and adding the same tag in one hint: the retraction wins, so
        // a hint can never both assert and drop a tag.
        let mut acc = ProjectionHint::default();
        let hint = ProjectionHint::default()
            .tag("binoc.move")
            .retract_tag("binoc.move");
        acc.merge_from(&hint);
        assert!(!acc.tags.contains(&"binoc.move".to_string()));
    }

    #[test]
    fn overlay_replaces_annotation_value_by_key() {
        let mut acc = ProjectionHint::default().annotate(
            "binoc",
            "content_type_inference",
            serde_json::json!("left"),
        );
        let overlay = ProjectionHint::default().annotate(
            "binoc",
            "content_type_inference",
            serde_json::json!("right"),
        );
        acc.overlay_from(&overlay);
        assert_eq!(
            acc.annotations,
            vec![Annotation::new(
                "binoc",
                "content_type_inference",
                serde_json::json!("right")
            )]
        );
    }
}
