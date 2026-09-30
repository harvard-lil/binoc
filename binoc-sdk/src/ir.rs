use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::types::{ArtifactDescriptor, ItemPair};

/// Which snapshot a [`Segment::Path`] resolves in.
///
/// Lets a renderer that can dereference a path — hyperlink it, shorten it
/// against a tree, show an icon — target the correct side of the diff
/// without understanding *why* the path appears (rename, copy,
/// cross-reference, ...). It is a property of the value, not an encoding of
/// any one concept. See ADR 2026-06-03-structured-summary-segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The "before" snapshot (a source/original path).
    From,
    /// The "after" snapshot (a destination/current path).
    To,
}

/// One piece of a [`Summary`].
///
/// Each variant carries a value *and*, implicitly, the render-time policy
/// for it: group an integer, format a float, leave text alone, dereference
/// a path. Renderers format by variant; they never parse prose to recover
/// the type of a value, because the producer never threw it away. Variants
/// track *render behavior*, not semantics — a currency or percent is `Text`
/// plus a number, never its own variant. See ADR
/// 2026-06-03-structured-summary-segments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Segment {
    /// Verbatim text: connective wording, units, punctuation, and any
    /// value the renderer must not reinterpret. Embedded digits are never
    /// reformatted — a number that should be grouped is a [`Segment::Uint`],
    /// and a path that could be linked is a [`Segment::Path`].
    Text(String),
    /// A path or locator. Renderers may shorten or hyperlink it; `snapshot`
    /// says which side of the diff it resolves in.
    Path { value: String, snapshot: Side },
    /// A non-negative count. Renderers apply digit grouping / locale.
    Uint(u64),
    /// A real-valued quantity. Renderers apply decimal / precision policy.
    Float(f64),
}

/// A structured, render-ready one-line summary: an ordered list of typed
/// [`Segment`]s.
///
/// Rule packs build it; renderers format
/// each segment by its type. This replaces free-text summaries so that
/// number and path formatting is a render-time decision the renderer makes
/// from typed values, rather than a fragile reparse of prose. A producer
/// that owns a concept (a rename detector) owns the *wording* — it emits the
/// connective `Text` and the `Path`s — while the renderer owns the
/// *typography*. See ADR 2026-06-03-structured-summary-segments.
///
/// The ergonomic shortcut for the common case is `impl Into<Summary>`:
/// `with_summary("plain text")` still works and produces a single
/// [`Segment::Text`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Summary(pub Vec<Segment>);

impl Summary {
    pub fn new() -> Self {
        Summary(Vec::new())
    }

    /// Append verbatim text, coalescing into a trailing text segment if the
    /// summary already ends in one. Keeps the serialized form canonical so
    /// that helpers like [`Summary::count`] which emit a count followed by
    /// text don't leave redundant adjacent text segments on the wire.
    pub fn text(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if let Some(Segment::Text(last)) = self.0.last_mut() {
            last.push_str(&value);
        } else {
            self.0.push(Segment::Text(value));
        }
        self
    }

    /// Append a non-negative count (renderer applies digit grouping).
    pub fn uint(mut self, value: u64) -> Self {
        self.0.push(Segment::Uint(value));
        self
    }

    /// Append a counted noun: `"{n} {noun}"`, with the count as a
    /// [`Segment::Uint`] (grouped by the renderer) and the noun pluralized
    /// with a trailing `s` unless `n == 1`. For irregular plurals, build the
    /// segments by hand. Example: `.count(5, "row")` -> `5 rows`.
    pub fn count(self, n: u64, noun: &str) -> Self {
        let suffix = if n == 1 { "" } else { "s" };
        self.uint(n).text(format!(" {noun}{suffix}"))
    }

    /// Append a real-valued quantity (renderer applies decimal policy).
    pub fn float(mut self, value: f64) -> Self {
        self.0.push(Segment::Float(value));
        self
    }

    /// Append a path/locator that resolves in `snapshot`.
    pub fn path(mut self, value: impl Into<String>, snapshot: Side) -> Self {
        self.0.push(Segment::Path {
            value: value.into(),
            snapshot,
        });
        self
    }

    /// Append a single segment.
    pub fn push(&mut self, segment: Segment) {
        self.0.push(segment);
    }

    /// Append all segments of another summary (e.g. when joining child
    /// summaries into a trailer).
    pub fn extend(&mut self, other: Summary) {
        self.0.extend(other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn segments(&self) -> &[Segment] {
        &self.0
    }

    /// Plain-text rendering with no formatting policy applied: text and path
    /// values verbatim, numbers in bare decimal form. For consumers without a
    /// renderer (Python bindings, machine sinks, provenance) and for internal
    /// bookkeeping such as path-statement detection.
    pub fn plain_text(&self) -> String {
        self.to_string()
    }

    /// Uppercase the first character of the leading text segment, if the
    /// summary begins with text. No-op when it begins with a number or path.
    /// Mirrors sentence-casing of prose without scanning a built string.
    pub fn capitalize_first(mut self) -> Self {
        if let Some(Segment::Text(text)) = self.0.first_mut() {
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
        }
        self
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.0 {
            match segment {
                Segment::Text(text) => f.write_str(text)?,
                Segment::Path { value, .. } => f.write_str(value)?,
                Segment::Uint(value) => write!(f, "{value}")?,
                Segment::Float(value) => write!(f, "{value}")?,
            }
        }
        Ok(())
    }
}

impl From<&str> for Summary {
    fn from(value: &str) -> Self {
        Summary(vec![Segment::Text(value.to_string())])
    }
}

impl From<String> for Summary {
    fn from(value: String) -> Self {
        Summary(vec![Segment::Text(value)])
    }
}

impl From<Vec<Segment>> for Summary {
    fn from(value: Vec<Segment>) -> Self {
        Summary(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Suggestion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: Summary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<ExtractHint>,
}

impl Diagnostic {
    pub fn new(
        severity: DiagnosticSeverity,
        code: impl Into<String>,
        message: impl Into<Summary>,
    ) -> Self {
        Self {
            severity,
            code: code.into(),
            message: message.into(),
            location: None,
            extract: None,
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<Summary>) -> Self {
        Self::new(DiagnosticSeverity::Warning, code, message)
    }

    pub fn error(code: impl Into<String>, message: impl Into<Summary>) -> Self {
        Self::new(DiagnosticSeverity::Error, code, message)
    }

    pub fn suggestion(code: impl Into<String>, message: impl Into<Summary>) -> Self {
        Self::new(DiagnosticSeverity::Suggestion, code, message)
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn with_extract_hint(mut self, hint: ExtractHint) -> Self {
        self.extract = Some(hint);
        self
    }

    fn normalized(mut self) -> Self {
        if self.location.as_deref().is_some_and(|s| s.is_empty()) {
            self.location = None;
        }
        self
    }
}

/// Renderer-visible metadata attached to a projected diff node by a rule pack.
///
/// Annotations are intentionally progressively typed: producers can start with
/// a string or simple JSON value, and renderers can either display the generic
/// value shape or add package/key-specific handling later. The package namespace
/// keeps independently-authored plugins from colliding on common keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Annotation {
    pub package: String,
    pub key: String,
    pub value: serde_json::Value,
}

impl Annotation {
    pub fn new(
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            package: package.into(),
            key: key.into(),
            value,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        self.value.as_str()
    }
}

/// Renderer-visible provenance for a projected diff node.
///
/// Most nodes have one source. Move and copy nodes use a `from` source whose
/// path differs from the projected node path; many-to-one projections such as
/// merges and deduplications carry multiple sources.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Source {
    /// Logical path of the source item.
    pub path: String,
    /// Snapshot side where `path` resolves.
    pub side: Side,
    /// Open evidence string from the rule/link that established provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// Open action associated with this source in the projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl Source {
    pub fn new(path: impl Into<String>, side: Side) -> Self {
        Self {
            path: path.into(),
            side,
            evidence: None,
            action: None,
        }
    }

    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence = Some(evidence.into());
        self
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }
}

/// A node in the projected diff tree — the durable changeset structure
/// consumed by renderers, serializers, and bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DiffNode {
    /// Open enum: "add", "remove", "modify", "move", "reorder",
    /// "schema_change", etc. Plugins may define new actions.
    pub action: String,

    /// Open string: "directory", "file", "tabular", "zip_archive", etc.
    /// No built-in types — conventions, not enforcement.
    pub item_type: String,

    /// Location within snapshot (logical path, including interior paths
    /// like "archive.zip/>data/file.csv"). `/>` marks a decompose boundary;
    /// a literal segment beginning with `>` is escaped as `\>`.
    pub path: String,

    /// Renderer-visible provenance for this projected node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Source>,

    /// Optional structured one-liner describing the change. Set during
    /// projection; renderers format each [`Segment`] by its
    /// type. Build it with [`Summary`]'s builder, or pass a plain string —
    /// `impl Into<Summary>` wraps it as a single [`Segment::Text`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,

    /// Open bag of semantic tags, namespaced by convention.
    /// e.g. "binoc.column-reorder", "biobinoc.gap-change"
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,

    /// Child diff nodes forming the tree structure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DiffNode>,

    /// Structured payload, schema determined by item_type/action convention.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, serde_json::Value>,

    /// Renderer-visible, structured evidence blocks. Rule packs populate
    /// these with bounded examples while they still have domain knowledge;
    /// renderers decide how much to display.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detail_blocks: Vec<DetailBlock>,

    /// Renderer-visible annotations supplied by rule packs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,

    /// The original item pair associated with this projected node when one is
    /// available. Session-scoped working data: available during a live run for
    /// rules and extractors that need to re-read source data. Callers writing
    /// changeset output must strip this via
    /// [`DiffNode::strip_transient`] before serializing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_items: Option<ItemPair>,

    /// Node-scoped diagnostics emitted during a run.
    /// Transient: the controller hoists them into [`Changeset::diagnostics`]
    /// at the end of the diff, then clears this field so the output shape
    /// stays as one durable top-level diagnostics list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,

    /// Published artifacts for this node. Session-scoped working data: carried
    /// across the plugin ABI wire as descriptors (the bytes live in the shared
    /// `data_root` cache), but not meaningful outside a session. Callers
    /// writing changeset output must strip this via
    /// [`DiffNode::strip_transient`] before serializing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDescriptor>,
}

impl DiffNode {
    pub fn new(
        action: impl Into<String>,
        item_type: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            item_type: item_type.into(),
            path: path.into(),
            sources: Vec::new(),
            summary: None,
            tags: BTreeSet::new(),
            children: Vec::new(),
            details: BTreeMap::new(),
            detail_blocks: Vec::new(),
            annotations: Vec::new(),
            source_items: None,
            diagnostics: Vec::new(),
            artifacts: Vec::new(),
        }
    }

    pub fn with_summary(mut self, summary: impl Into<Summary>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.insert(tag.into());
        self
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    pub fn with_children(mut self, children: Vec<DiffNode>) -> Self {
        self.children = children;
        self
    }

    pub fn with_detail_block(mut self, block: DetailBlock) -> Self {
        self.detail_blocks.push(block);
        self
    }

    pub fn with_annotation_from(
        mut self,
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.annotate_from(package, key, value);
        self
    }

    pub fn with_source(mut self, source: Source) -> Self {
        self.push_source(source);
        self
    }

    pub fn with_sources(mut self, sources: Vec<Source>) -> Self {
        self.sources = sources;
        self.normalize_sources();
        self
    }

    pub fn with_source_items(mut self, items: ItemPair) -> Self {
        self.source_items = Some(items);
        self
    }

    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.push_diagnostic(diagnostic);
        self
    }

    pub fn with_artifact(mut self, artifact: ArtifactDescriptor) -> Self {
        self.artifacts.push(artifact);
        self
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        let diagnostic = if diagnostic.location.is_none() && !self.path.is_empty() {
            diagnostic.with_location(self.path.clone())
        } else {
            diagnostic
        };
        self.diagnostics.push(diagnostic.normalized());
    }

    pub fn push_source(&mut self, source: Source) {
        self.sources.push(source);
        self.normalize_sources();
    }

    pub fn primary_from_source(&self) -> Option<&Source> {
        self.sources.iter().find(|source| source.side == Side::From)
    }

    fn normalize_sources(&mut self) {
        self.sources.sort();
        self.sources.dedup();
    }

    pub fn annotate_from(
        &mut self,
        package: impl Into<String>,
        key: impl Into<String>,
        value: serde_json::Value,
    ) {
        let package = package.into();
        let key = key.into();
        if let Some(existing) = self
            .annotations
            .iter_mut()
            .find(|annotation| annotation.package == package && annotation.key == key)
        {
            existing.value = value;
        } else {
            self.annotations.push(Annotation::new(package, key, value));
        }
    }

    pub fn annotation(&self, package: &str, key: &str) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|annotation| annotation.package == package && annotation.key == key)
    }

    pub fn binoc_annotation(&self, key: &str) -> Option<&Annotation> {
        self.annotation("binoc", key)
    }

    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }

    pub fn all_tags(&self) -> BTreeSet<String> {
        let mut tags = self.tags.clone();
        for child in &self.children {
            tags.extend(child.all_tags());
        }
        tags
    }

    fn drain_diagnostics_into(&mut self, target: &mut Vec<Diagnostic>) {
        target.append(&mut self.diagnostics);
        for child in &mut self.children {
            child.drain_diagnostics_into(target);
        }
    }

    /// Recursively clear session-scoped transient fields (`source_items`,
    /// `diagnostics`, `artifacts`) on this node and all descendants.
    ///
    /// These fields are wire-visible so the plugin ABI can move them across
    /// process-ready boundaries, but they are not meaningful outside a live
    /// session and must be stripped before writing changeset output intended
    /// for users (JSON files, renderer output, Python return values).
    pub fn strip_transient(&mut self) {
        self.source_items = None;
        self.diagnostics.clear();
        self.artifacts.clear();
        for child in &mut self.children {
            child.strip_transient();
        }
    }
}

/// Reserved run-scoped claim slot.
///
/// The shape is intentionally provisional pending the CFM-41 global-claim
/// prototype. It gives renderers and serialized changesets a stable place for
/// non-tree claims without committing the claim vocabulary yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct GlobalClaim {
    /// Open claim verb for a renderer- or plugin-defined run-scoped claim.
    pub verb: String,
    /// Claim-specific structured parameters.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
    /// Optional renderer-facing summary for the claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
}

impl GlobalClaim {
    pub fn new(verb: impl Into<String>) -> Self {
        Self {
            verb: verb.into(),
            params: BTreeMap::new(),
            summary: None,
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    pub fn with_summary(mut self, summary: impl Into<Summary>) -> Self {
        self.summary = Some(summary.into());
        self
    }
}

/// Renderer-visible, bounded evidence attached to a diff node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DetailBlock {
    /// Stable within this node, for anchors and extract selection.
    pub id: String,
    /// Open, namespaced kind such as `binoc.tabular.cell_changes.v1`.
    pub kind: String,
    /// Short renderer-facing label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Total matching items if known, including omitted examples.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_count: Option<u64>,
    /// Captured examples for inline rendering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<DetailExample>,
    /// Named extract aspects for exhaustive retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extract: Vec<ExtractHint>,
    /// Whether the producer truncated capture before exhausting candidates.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl DetailBlock {
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            label: None,
            total_count: None,
            examples: Vec::new(),
            extract: Vec::new(),
            truncated: false,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_total_count(mut self, total_count: u64) -> Self {
        self.total_count = Some(total_count);
        self
    }

    pub fn with_example(mut self, example: DetailExample) -> Self {
        self.examples.push(example);
        self
    }

    pub fn with_extract_hint(mut self, hint: ExtractHint) -> Self {
        self.extract.push(hint);
        self
    }
}

/// One bounded example inside a detail block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DetailExample {
    /// Structured locator such as row/column, line range, or key path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub locator: BTreeMap<String, serde_json::Value>,
    /// Value before the change, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<ValuePreview>,
    /// Value after the change, if present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<ValuePreview>,
    /// Domain-specific structured context.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl DetailExample {
    pub fn new() -> Self {
        Self {
            locator: BTreeMap::new(),
            before: None,
            after: None,
            fields: BTreeMap::new(),
        }
    }
}

impl Default for DetailExample {
    fn default() -> Self {
        Self::new()
    }
}

/// A bounded preview of one value in a detail example.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ValuePreview {
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Pointer to an extract aspect that can return exhaustive content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExtractHint {
    /// Aspect name accepted by `binoc extract`.
    pub aspect: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ExtractHint {
    pub fn new(aspect: impl Into<String>) -> Self {
        Self {
            aspect: aspect.into(),
            changeset_path: None,
            label: None,
        }
    }

    pub fn with_changeset_path(mut self, path: impl Into<String>) -> Self {
        self.changeset_path = Some(path.into());
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    fn fill_changeset_path_if_missing(&mut self, path: &str) {
        if self.changeset_path.is_none() {
            self.changeset_path = Some(path.to_string());
        }
    }
}

/// A structured description of how to get from one snapshot to the next.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Changeset {
    pub from_snapshot: String,
    pub to_snapshot: String,
    /// Run-scoped claims that do not belong to one tree node.
    ///
    /// Reserved for the CFM-41 global-claim prototype; empty in current engine
    /// output.
    #[serde(default)]
    pub claims: Vec<GlobalClaim>,
    pub root: Option<DiffNode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl Changeset {
    pub fn new(from: impl Into<String>, to: impl Into<String>, root: Option<DiffNode>) -> Self {
        Self {
            from_snapshot: from.into(),
            to_snapshot: to.into(),
            claims: Vec::new(),
            root,
            metadata: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.root.as_ref().map_or(0, |r| r.node_count())
    }

    pub fn push_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic.normalized());
    }

    pub fn hoist_node_diagnostics(&mut self) {
        if let Some(root) = self.root.as_mut() {
            root.drain_diagnostics_into(&mut self.diagnostics);
        }
    }

    pub fn dedupe_and_cap_diagnostics(&mut self, max_diagnostics: usize) {
        let mut seen: BTreeSet<(String, Option<String>)> = BTreeSet::new();
        let mut deduped = Vec::with_capacity(self.diagnostics.len().min(max_diagnostics));

        for diagnostic in self.diagnostics.drain(..).map(Diagnostic::normalized) {
            let key = (diagnostic.code.clone(), diagnostic.location.clone());
            if seen.insert(key) {
                deduped.push(diagnostic);
                if deduped.len() >= max_diagnostics {
                    break;
                }
            }
        }

        self.diagnostics = deduped;
    }

    pub fn fill_missing_extract_hint_paths(&mut self, path: impl AsRef<str>) {
        let path = path.as_ref();
        for diagnostic in &mut self.diagnostics {
            diagnostic.fill_changeset_path_if_missing(path);
        }
        if let Some(root) = self.root.as_mut() {
            root.fill_changeset_path_if_missing(path);
        }
    }

    /// Recursively clear session-scoped transient fields on the root and all
    /// descendants. See [`DiffNode::strip_transient`].
    pub fn strip_transient(&mut self) {
        if let Some(root) = self.root.as_mut() {
            root.strip_transient();
        }
    }
}

impl Diagnostic {
    fn fill_changeset_path_if_missing(&mut self, path: &str) {
        if let Some(extract) = self.extract.as_mut() {
            extract.fill_changeset_path_if_missing(path);
        }
    }
}

impl DetailBlock {
    fn fill_changeset_path_if_missing(&mut self, path: &str) {
        for extract in &mut self.extract {
            extract.fill_changeset_path_if_missing(path);
        }
    }
}

impl DiffNode {
    fn fill_changeset_path_if_missing(&mut self, path: &str) {
        for detail_block in &mut self.detail_blocks {
            detail_block.fill_changeset_path_if_missing(path);
        }
        for diagnostic in &mut self.diagnostics {
            diagnostic.fill_changeset_path_if_missing(path);
        }
        for child in &mut self.children {
            child.fill_changeset_path_if_missing(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_node_new_creates_node_with_correct_fields() {
        let node = DiffNode::new("modify", "file", "path/to/file.csv");
        assert_eq!(node.action, "modify");
        assert_eq!(node.item_type, "file");
        assert_eq!(node.path, "path/to/file.csv");
        assert!(node.sources.is_empty());
        assert!(node.tags.is_empty());
        assert!(node.children.is_empty());
        assert!(node.details.is_empty());
        assert!(node.detail_blocks.is_empty());
        assert!(node.annotations.is_empty());
    }

    #[test]
    fn diff_node_builder_methods_chain_correctly() {
        let child = DiffNode::new("add", "file", "child.txt");
        let node = DiffNode::new("modify", "directory", "dir")
            .with_tag("binoc.column-reorder")
            .with_tag("binoc.whitespace")
            .with_detail("lines_changed", serde_json::json!(42))
            .with_annotation_from("binoc", "note", serde_json::json!("check distribution"))
            .with_children(vec![child])
            .with_source(Source::new("old/dir", Side::From).with_action("move"));

        assert_eq!(node.tags.len(), 2);
        assert!(node.tags.contains("binoc.column-reorder"));
        assert!(node.tags.contains("binoc.whitespace"));
        assert_eq!(
            node.details.get("lines_changed"),
            Some(&serde_json::json!(42))
        );
        assert_eq!(
            node.binoc_annotation("note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("check distribution"))
        );
        assert!(node.detail_blocks.is_empty());
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].path, "child.txt");
        assert_eq!(node.sources.len(), 1);
        assert_eq!(node.sources[0].path, "old/dir");
        assert_eq!(node.sources[0].side, Side::From);
    }

    #[test]
    fn annotations_are_namespaced_and_replace_by_package_key() {
        let mut node = DiffNode::new("modify", "file", "data.csv");
        node.annotate_from("binoc", "note", serde_json::json!("first"));
        node.annotate_from("binoc", "note", serde_json::json!("second"));
        node.annotate_from("example.plugin", "note", serde_json::json!("external"));

        assert_eq!(node.annotations.len(), 2);
        assert_eq!(
            node.binoc_annotation("note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("second"))
        );
        assert_eq!(
            node.annotation("example.plugin", "note")
                .map(|annotation| &annotation.value),
            Some(&serde_json::json!("external"))
        );
    }

    #[test]
    fn node_count_leaf_returns_one() {
        let node = DiffNode::new("add", "file", "file.txt");
        assert_eq!(node.node_count(), 1);
    }

    #[test]
    fn node_count_tree_returns_correct_total() {
        let node = DiffNode::new("modify", "dir", "dir").with_children(vec![
            DiffNode::new("add", "file", "a.txt"),
            DiffNode::new("modify", "dir", "sub").with_children(vec![DiffNode::new(
                "remove",
                "file",
                "sub/b.txt",
            )]),
        ]);
        assert_eq!(node.node_count(), 4);
    }

    #[test]
    fn all_tags_collects_from_entire_subtree() {
        let node = DiffNode::new("modify", "dir", "dir")
            .with_tag("root-tag")
            .with_children(vec![
                DiffNode::new("add", "file", "a").with_tag("child-tag"),
                DiffNode::new("remove", "file", "b")
                    .with_children(vec![
                        DiffNode::new("modify", "file", "c").with_tag("grandchild-tag")
                    ]),
            ]);
        let tags = node.all_tags();
        assert_eq!(tags.len(), 3);
        assert!(tags.contains("root-tag"));
        assert!(tags.contains("child-tag"));
        assert!(tags.contains("grandchild-tag"));
    }

    #[test]
    fn serde_round_trip_preserves_equality() {
        let node = DiffNode::new("move", "file", "new/path.csv")
            .with_tag("binoc.move")
            .with_detail("distance", serde_json::json!(10))
            .with_detail_block(
                DetailBlock::new("changed_cells", "binoc.tabular.cell_changes.v1")
                    .with_label("Changed cells")
                    .with_total_count(1)
                    .with_example(DetailExample {
                        locator: BTreeMap::from([
                            ("row".into(), serde_json::json!(1)),
                            ("column".into(), serde_json::json!("status")),
                        ]),
                        before: Some(ValuePreview {
                            value: serde_json::json!("draft"),
                            media_type: Some("text/plain".into()),
                            truncated: false,
                        }),
                        after: Some(ValuePreview {
                            value: serde_json::json!("published"),
                            media_type: Some("text/plain".into()),
                            truncated: false,
                        }),
                        fields: BTreeMap::new(),
                    })
                    .with_extract_hint(
                        ExtractHint::new("cells_changed").with_label("All changed cells"),
                    ),
            )
            .with_source(Source::new("old/path.csv", Side::From).with_action("move"));
        let json = serde_json::to_string(&node).unwrap();
        let restored: DiffNode = serde_json::from_str(&json).unwrap();
        assert_eq!(node.action, restored.action);
        assert_eq!(node.item_type, restored.item_type);
        assert_eq!(node.path, restored.path);
        assert_eq!(node.sources, restored.sources);
        assert_eq!(node.tags, restored.tags);
        assert_eq!(node.details, restored.details);
        assert_eq!(restored.detail_blocks.len(), 1);
        assert_eq!(restored.detail_blocks[0].examples.len(), 1);
    }

    #[test]
    fn changeset_construction_and_node_count() {
        let root = DiffNode::new("modify", "dir", "root").with_children(vec![
            DiffNode::new("add", "file", "root/a.txt"),
            DiffNode::new("remove", "file", "root/b.txt"),
        ]);
        let changeset = Changeset::new("v1", "v2", Some(root));
        assert_eq!(changeset.from_snapshot, "v1");
        assert_eq!(changeset.to_snapshot, "v2");
        assert!(changeset.claims.is_empty());
        assert_eq!(changeset.node_count(), 3);
    }

    #[test]
    fn transient_fields_round_trip_through_serde() {
        // Session-scoped transient fields (`source_items`, `artifacts`,
        // `diagnostics`) are wire-visible so the plugin ABI can carry them
        // across a (potentially process-isolated) boundary.
        use crate::types::{
            ArtifactDescriptor, ArtifactFormat, ArtifactSubject, ItemPair, ItemRef,
        };

        let artifact = ArtifactDescriptor {
            format: ArtifactFormat::new("binoc", "tabular", 1),
            subject: ArtifactSubject::Pair,
            producer: "binoc.csv".into(),
            handle: "cache/tabular-abc123".into(),
        };
        let source_items = ItemPair::both(
            ItemRef {
                logical_path: "data.csv".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                projection_hint: Default::default(),
                tabular_parse: None,
                handle: "/tmp/a/data.csv".into(),
            },
            ItemRef {
                logical_path: "data.csv".into(),
                is_dir: false,
                content_hash: None,
                size: None,
                media_type: None,
                projection_hint: Default::default(),
                tabular_parse: None,
                handle: "/tmp/b/data.csv".into(),
            },
        );
        let child = DiffNode::new("modify", "tabular", "dir/data.csv")
            .with_artifact(artifact.clone())
            .with_source_items(source_items.clone())
            .with_diagnostic(
                Diagnostic::suggestion("binoc.demo", "Try a richer plugin")
                    .with_extract_hint(ExtractHint::new("content")),
            );
        let root = DiffNode::new("modify", "directory", "dir").with_children(vec![child]);

        let json = serde_json::to_string(&root).unwrap();
        let restored: DiffNode = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.children.len(), 1);
        let restored_child = &restored.children[0];
        assert_eq!(restored_child.artifacts.len(), 1, "child artifact missing");
        assert_eq!(restored_child.artifacts[0].handle, artifact.handle);
        assert!(
            restored_child.source_items.is_some(),
            "child source_items missing"
        );
        assert_eq!(restored_child.diagnostics.len(), 1);
        assert_eq!(
            restored_child.diagnostics[0]
                .extract
                .as_ref()
                .map(|hint| hint.aspect.as_str()),
            Some("content")
        );
    }

    #[test]
    fn hoisted_diagnostics_are_deduped_and_capped() {
        let mut root = DiffNode::new("modify", "directory", "");
        root.push_diagnostic(Diagnostic::suggestion(
            "binoc.binary-fallback",
            "Try a plugin",
        ));
        root.push_diagnostic(Diagnostic::suggestion(
            "binoc.binary-fallback",
            "Try a plugin",
        ));
        root.children = vec![
            DiffNode::new("modify", "file", "a.bin").with_diagnostic(Diagnostic::suggestion(
                "binoc.binary-fallback",
                "Try a plugin",
            )),
            DiffNode::new("modify", "file", "b.bin")
                .with_diagnostic(Diagnostic::warning("binoc.other", "Other issue")),
        ];

        let mut changeset = Changeset::new("a", "b", Some(root));
        changeset.hoist_node_diagnostics();
        changeset.dedupe_and_cap_diagnostics(2);

        assert_eq!(changeset.diagnostics.len(), 2);
        assert_eq!(changeset.diagnostics[0].code, "binoc.binary-fallback");
        assert_eq!(changeset.diagnostics[0].location, None);
        assert_eq!(changeset.diagnostics[1].location.as_deref(), Some("a.bin"));
    }

    #[test]
    fn strip_transient_clears_every_descendant() {
        use crate::types::{ArtifactDescriptor, ArtifactFormat, ArtifactSubject};
        let artifact = ArtifactDescriptor {
            format: ArtifactFormat::new("binoc", "tabular", 1),
            subject: ArtifactSubject::Pair,
            producer: "binoc.csv".into(),
            handle: "h".into(),
        };
        let grandchild = DiffNode::new("modify", "tabular", "a/b/c.csv")
            .with_artifact(artifact)
            .with_diagnostic(Diagnostic::warning("binoc.test", "test"));
        let child = DiffNode::new("modify", "directory", "a/b").with_children(vec![grandchild]);
        let mut root = DiffNode::new("modify", "directory", "a").with_children(vec![child]);
        root.strip_transient();
        fn all_empty(n: &DiffNode) -> bool {
            n.artifacts.is_empty()
                && n.diagnostics.is_empty()
                && n.source_items.is_none()
                && n.children.iter().all(all_empty)
        }
        assert!(all_empty(&root));
    }

    #[test]
    fn changeset_node_count_none_root() {
        let changeset = Changeset::new("v1", "v2", None);
        assert_eq!(changeset.node_count(), 0);
    }

    #[test]
    fn fill_missing_extract_hint_paths_updates_nested_hints_only_when_missing() {
        let child = DiffNode::new("modify", "file", "child.csv")
            .with_detail_block(
                DetailBlock::new("cells", "binoc.tabular.cell_changes.v1")
                    .with_extract_hint(ExtractHint::new("cells_changed")),
            )
            .with_diagnostic(
                Diagnostic::warning("binoc.child", "child diagnostic")
                    .with_extract_hint(ExtractHint::new("content")),
            );
        let root = DiffNode::new("modify", "file", "root.csv")
            .with_detail_block(
                DetailBlock::new("rows", "binoc.tabular.row_changes.v1").with_extract_hint(
                    ExtractHint::new("rows_changed").with_changeset_path("already-set.json"),
                ),
            )
            .with_children(vec![child]);
        let mut changeset = Changeset::new("v1", "v2", Some(root));
        changeset.push_diagnostic(
            Diagnostic::warning("binoc.root", "top diagnostic")
                .with_extract_hint(ExtractHint::new("content")),
        );

        changeset.fill_missing_extract_hint_paths("changeset.json");

        assert_eq!(
            changeset.diagnostics[0]
                .extract
                .as_ref()
                .and_then(|hint| hint.changeset_path.as_deref()),
            Some("changeset.json")
        );
        let root = changeset.root.as_ref().unwrap();
        assert_eq!(
            root.detail_blocks[0].extract[0].changeset_path.as_deref(),
            Some("already-set.json")
        );
        let child = &root.children[0];
        assert_eq!(
            child.detail_blocks[0].extract[0].changeset_path.as_deref(),
            Some("changeset.json")
        );
        assert_eq!(
            child.diagnostics[0]
                .extract
                .as_ref()
                .and_then(|hint| hint.changeset_path.as_deref()),
            Some("changeset.json")
        );
    }
}
