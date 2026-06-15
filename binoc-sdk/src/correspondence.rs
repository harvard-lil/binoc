use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    ArtifactFormat, BinocResult, DataAccess, Diagnostic, ExtractResult, IdentityFailurePolicy,
    ItemRef, Segment, Summary,
};

/// Which side tree a node belongs to in the correspondence-first IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeId {
    pub side: TreeSide,
    pub index: u32,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
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

    pub fn summary(mut self, summary: impl Into<Summary>) -> Self {
        self.summary = Some(summary.into());
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
        self.tags.extend(other.tags.iter().cloned());
        self.tags.sort();
        self.tags.dedup();
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
        self.tags.extend(other.tags.iter().cloned());
        self.tags.sort();
        self.tags.dedup();
    }
}

pub struct ProjectionAnnotationContext<'a> {
    pub action: &'a str,
    pub item_type: &'a str,
    pub path: &'a str,
    pub source_path: Option<&'a str>,
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
    pub row_keys: BTreeMap<String, Vec<String>>,
    pub row_identity_policies: BTreeMap<String, RowIdentityPolicies>,
    pub root_projection: ProjectionHint,
    pub dataset_configurator: Option<Arc<dyn CorrespondenceDatasetConfigurator>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParseDescriptor {
    pub name: String,
    pub input: NodeMatch,
    pub output: ArtifactFormat,
    #[serde(default)]
    pub fires_beneath_settled: bool,
}

pub trait ParseRule: Send + Sync {
    fn descriptor(&self) -> ParseDescriptor;
    fn parse(&self, item: &ItemRef, data: &dyn DataAccess) -> BinocResult<ParseOutput>;
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
    pub row_keys: &'a [String],
    pub row_identity_policies: RowIdentityPolicies,
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
