use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ir::DiffNode;

// ── Artifact types ──────────────────────────────────────────────────

/// Which side of a comparison an artifact describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ArtifactSubject {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "pair")]
    Pair,
}

/// Identifies an artifact's data format as a structured tuple of
/// (package, name, version).
///
/// - **`package`** — the package that owns and defines this format,
///   resolvable through the language's normal package system
///   (e.g. `"binoc"`, `"binoc-csv"`, `"acme-parquet"`).
/// - **`name`** — the format name within that package
///   (e.g. `"tabular"`, `"relational-schema"`).
/// - **`version`** — a single integer. Bump only for breaking schema
///   changes. Adding optional fields to an existing version is fine
///   and does not require a bump (JSON/serde naturally ignore unknown
///   fields and default missing ones).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArtifactFormat {
    pub package: String,
    pub name: String,
    pub version: u32,
}

impl ArtifactFormat {
    pub fn new(package: impl Into<String>, name: impl Into<String>, version: u32) -> Self {
        Self {
            package: package.into(),
            name: name.into(),
            version,
        }
    }
}

impl std::fmt::Display for ArtifactFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.v{}", self.package, self.name, self.version)
    }
}

/// Descriptor for a published artifact attached to a node.
///
/// Artifacts are the unified mechanism for both private reuse and
/// cross-plugin composition. Parse rules publish artifacts; downstream rules
/// consume them by format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArtifactDescriptor {
    pub format: ArtifactFormat,
    pub subject: ArtifactSubject,
    pub producer: String,
    /// Opaque handle managed by the SDK's DataAccess implementation.
    /// Plugins should not create or interpret this value directly.
    pub handle: String,
}

// ── Standard artifact formats ───────────────────────────────────────

/// Standard format for tabular data artifacts.
///
/// Any parser for a tabular source format (CSV, TSV, Excel, Parquet, ...)
/// should publish artifacts with this format so that generic tabular writers,
/// compaction rules, and extractors can consume them without
/// knowing the source format.
pub fn tabular_v1() -> ArtifactFormat {
    ArtifactFormat::new("binoc", "tabular", 1)
}

/// Standard format for a generic, format-neutral value tree.
///
/// Produced by parsers for tree-structured formats (JSON, JSONL of mixed shape,
/// YAML, TOML, ...) and consumed by the structured-document writer. This is the
/// fallback for any structured source that is not a consistently-shaped record
/// collection. See the typed-record ADR.
pub fn structured_document_v1() -> ArtifactFormat {
    ArtifactFormat::new("binoc", "structured_document", 1)
}

/// Standard format for tier-3 *parser metadata* — facts a parser extracted about
/// a node that are not the node's primary data payload: source-format identity
/// and version, file-level properties, cross-table dictionaries, creator/tooling
/// provenance. Rides as a second artifact on the parsed node (alongside a
/// `tabular_v1` leaf, or on a multi-table container that has no table of its
/// own). Consumed by format, like any artifact; carrying it is useful even with
/// no current consumer (see the tiered-artifact-metadata ADR).
pub fn parser_metadata_v1() -> ArtifactFormat {
    ArtifactFormat::new("binoc", "parser_metadata", 1)
}

// ── Cell value model ────────────────────────────────────────────────

static NULL_VALUE: Value = Value::Null;

/// A single tabular cell value.
///
/// Scalars (`Null`/`Bool`/`Number`/`String`) diff by content. `Nested` holds a
/// canonicalized object/array (object keys sorted recursively) and participates
/// in diffs by equality only — a changed nested cell is reported as a cell edit,
/// but binoc does not recurse into it (see the typed-record ADR). `String` cells
/// are the all-untyped case used by CSV and other typeless sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Nested(Box<serde_json::Value>),
}

impl Value {
    /// Build a cell value from arbitrary JSON, canonicalizing nested containers
    /// so that equality is order-independent.
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => Value::Number(n),
            serde_json::Value::String(s) => Value::String(s),
            other => Value::Nested(Box::new(canonicalize_json(other))),
        }
    }

    /// The JSON representation of this cell, used when building edit params.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Number(n) => serde_json::Value::Number(n.clone()),
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Nested(v) => (**v).clone(),
        }
    }

    /// A flat textual rendering for tokenization, CSV serialization, and scoring.
    pub fn as_text(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Value::Null => std::borrow::Cow::Borrowed(""),
            Value::Bool(true) => std::borrow::Cow::Borrowed("true"),
            Value::Bool(false) => std::borrow::Cow::Borrowed("false"),
            Value::Number(n) => std::borrow::Cow::Owned(n.to_string()),
            Value::String(s) => std::borrow::Cow::Borrowed(s.as_str()),
            Value::Nested(v) => std::borrow::Cow::Owned(v.to_string()),
        }
    }

    /// True when the value carries no content for keying/identity purposes
    /// (null, or an empty/whitespace string).
    pub fn is_blank(&self) -> bool {
        match self {
            Value::Null => true,
            Value::String(s) => s.trim().is_empty(),
            _ => false,
        }
    }

    /// Feed a stable, type-tagged byte signature into a hasher (row alignment).
    pub fn hash_into(&self, hasher: &mut blake3::Hasher) {
        match self {
            Value::Null => {
                hasher.update(&[0]);
            }
            Value::Bool(b) => {
                hasher.update(&[1, *b as u8]);
            }
            Value::Number(n) => {
                hasher.update(&[2]);
                hasher.update(n.to_string().as_bytes());
            }
            Value::String(s) => {
                hasher.update(&[3]);
                hasher.update(&(s.len() as u64).to_le_bytes());
                hasher.update(s.as_bytes());
            }
            Value::Nested(v) => {
                hasher.update(&[4]);
                hasher.update(v.to_string().as_bytes());
            }
        }
    }
}

impl Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Value::from_json(serde_json::Value::deserialize(
            deserializer,
        )?))
    }
}

/// Recursively sort object keys so that nested-value equality is order-independent.
fn canonicalize_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize_json).collect())
        }
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| (k, canonicalize_json(v)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        other => other,
    }
}

// ── Format-neutral data types ───────────────────────────────────────

/// Format-neutral tabular data: an ordered list of records with a shared column
/// schema. Produced by CSV, JSON record arrays, JSONL, Excel, Parquet, DB, and
/// other tabular parsers; consumed by tabular writers, compaction rules, and
/// extractors.
///
/// This is the codec type for the [`tabular_v1`] artifact format.
/// Serialize with `serde_json::to_vec`, deserialize with `serde_json::from_slice`.
///
/// The shape spectrum (rectangular?, named columns?, typed cells?) is *derived*
/// from the data via [`TabularData::is_rectangular`],
/// [`TabularData::has_named_columns`], and the cell `Value` variants — rules gate
/// their behavior on those facts rather than on artifact subtypes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabularData {
    /// Column names. For headerless sources these are synthesized positional
    /// labels and `has_header` is `false`.
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Whether the source supplied real column names (CSV header, object keys).
    #[serde(default = "default_true")]
    pub has_header: bool,
    /// Declared identity column names, in order. Empty when the source declares
    /// no key; drives keyed row alignment when present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub key: Vec<String>,
    /// Optional source-declared type per column (parallel to `headers`), when the
    /// source format carries one (DB, Parquet, Stata). Empty means "none".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_types: Vec<Option<String>>,
    /// Optional per-column metadata bag (parallel to `headers`), when the source
    /// format carries column-scoped facts a generic tabular consumer would not
    /// otherwise see — labels, display formats, value-label set names, units.
    /// Each entry is an open object (or `Null` for a column with no metadata).
    /// Empty means "none". This is tier 1 of the tiered-metadata design (see the
    /// tiered-artifact-metadata ADR): facts keyed to a *column*.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_metadata: Vec<serde_json::Value>,
    /// Optional table-scoped metadata bag — facts about *this table as a whole*
    /// that are not per-column and not per-file (a single-table file folds its
    /// source-format facts here; a table inside a multi-table container carries
    /// only its own facts, e.g. dataset name/label). `Null` means "none". This
    /// is tier 2 of the tiered-metadata design.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub table_metadata: serde_json::Value,
}

fn default_true() -> bool {
    true
}

impl TabularData {
    /// Construct from all-string cells (CSV and other untyped sources). Cells are
    /// wrapped in [`Value::String`]; the result is byte-identical in behavior to
    /// the legacy all-string tabular model.
    pub fn from_string_rows(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            headers,
            rows: rows
                .into_iter()
                .map(|row| row.into_iter().map(Value::String).collect())
                .collect(),
            has_header: true,
            key: Vec::new(),
            column_types: Vec::new(),
            column_metadata: Vec::new(),
            table_metadata: serde_json::Value::Null,
        }
    }

    /// Construct from typed rows with a named header.
    pub fn new(headers: Vec<String>, rows: Vec<Vec<Value>>) -> Self {
        Self {
            headers,
            rows,
            has_header: true,
            key: Vec::new(),
            column_types: Vec::new(),
            column_metadata: Vec::new(),
            table_metadata: serde_json::Value::Null,
        }
    }

    /// Attach tier-1 per-column metadata (parallel to `headers`). Builder-style
    /// so producers can enrich a table without restating every field.
    pub fn with_column_metadata(mut self, column_metadata: Vec<serde_json::Value>) -> Self {
        self.column_metadata = column_metadata;
        self
    }

    /// Attach tier-2 table-scoped metadata.
    pub fn with_table_metadata(mut self, table_metadata: serde_json::Value) -> Self {
        self.table_metadata = table_metadata;
        self
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

    pub fn column_values(&self, name: &str) -> Option<Vec<&Value>> {
        let idx = self.column_index(name)?;
        Some(
            self.rows
                .iter()
                .map(|r| r.get(idx).unwrap_or(&NULL_VALUE))
                .collect(),
        )
    }

    /// Every row has arity equal to the column count.
    pub fn is_rectangular(&self) -> bool {
        let width = self.headers.len();
        self.rows.iter().all(|row| row.len() == width)
    }

    /// The source supplied real, usable column names.
    pub fn has_named_columns(&self) -> bool {
        self.has_header && !self.headers.is_empty()
    }

    /// Columns can be identified across rows and snapshots — the precondition for
    /// cell-grain and column-grain edits. Otherwise the writer degrades to
    /// row-grain output.
    pub fn stable_columns(&self) -> bool {
        self.has_named_columns() || self.is_rectangular()
    }

    pub fn to_csv(&self) -> String {
        let mut out = self.headers.join(",");
        out.push('\n');
        for row in &self.rows {
            let cells: Vec<String> = row.iter().map(|v| v.as_text().into_owned()).collect();
            out.push_str(&cells.join(","));
            out.push('\n');
        }
        out
    }
}

/// Generic format-neutral value tree. Codec type for [`structured_document_v1`].
///
/// All source formats transcode their content into a single `serde_json::Value`
/// tree; `format` records the origin ("json", "yaml", "toml", ...) and `source`
/// is an open bag of serialization facts (key order, indentation, BOM, trailing
/// newline) that consumers ignore when unknown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredDocument {
    pub value: serde_json::Value,
    pub format: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub source: serde_json::Value,
}

/// Codec type for [`parser_metadata_v1`] — tier-3 parser metadata.
///
/// `format` is the producer's source-format identity (e.g. `"stata_dta"`,
/// `"sas7bdat"`, `"sas_xport"`), so a consumer can interpret `value` without
/// guessing. `value` is an open bag of parser-level facts; consumers diff it
/// generically and ignore keys they do not recognize. Deliberately flat: this
/// is "a matching subtype for a record artifact" today, and may grow typed
/// structure in a future version rather than via artifact-format inheritance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParserMetadata {
    pub format: String,
    pub value: serde_json::Value,
}

impl ParserMetadata {
    pub fn new(format: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            format: format.into(),
            value,
        }
    }
}

// ── Dataset semantics config ───────────────────────────────────────

/// SDK-owned dataset semantics section shared by plugins.
///
/// Hosts pass this through unchanged; plugins deserialize the parts they
/// understand. The schema is intentionally conservative in v1.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetSemanticsV1 {
    #[serde(default)]
    pub defaults: DatasetDefaults,
    #[serde(default)]
    pub paths: Vec<PathConfigEntry>,
    #[serde(default)]
    pub files: FileIdentityConfig,
    #[serde(default)]
    pub tables: TableConfig,
    #[serde(default)]
    pub correspondence: CorrespondenceConfig,
    #[serde(default)]
    pub reduced_precision: ReducedPrecisionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReducedPrecisionConfig {
    /// String sentinels that represent a suppressed published value after
    /// reduced-precision post-processing. The empty string covers both blank
    /// cells and `null`, preserving the historical blank/null sentinel.
    #[serde(default = "default_suppression_sentinels")]
    pub suppression_sentinels: Vec<String>,
}

impl Default for ReducedPrecisionConfig {
    fn default() -> Self {
        Self {
            suppression_sentinels: default_suppression_sentinels(),
        }
    }
}

fn default_suppression_sentinels() -> Vec<String> {
    vec!["*".into(), "(D)".into(), "(S)".into(), "".into()]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetDefaults {
    #[serde(default)]
    pub row_identity: RowIdentity,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeIdentity {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_attribute: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PathConfigEntry {
    #[serde(default, rename = "match")]
    pub match_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<TabularShapeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<CsvDialectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_identity: Option<RowIdentityPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_identity: Option<NodeIdentity>,
    #[serde(skip)]
    pub unknown_fields: Vec<String>,
}

impl<'de> Deserialize<'de> for PathConfigEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Default, Deserialize)]
        struct RawPathConfigEntry {
            #[serde(default, rename = "match")]
            match_: String,
            #[serde(default)]
            content_type: Option<String>,
            #[serde(default)]
            rule: Option<String>,
            #[serde(default)]
            dialect: Option<CsvDialectConfig>,
            #[serde(default)]
            shape: Option<TabularShapeConfig>,
            #[serde(default)]
            records_path: Option<String>,
            #[serde(default)]
            row_identity: Option<RowIdentityPatch>,
            #[serde(default)]
            node_identity: Option<NodeIdentity>,
            #[serde(default)]
            columns: Vec<String>,
            #[serde(default)]
            on_null_key: Option<IdentityFailurePolicy>,
            #[serde(default)]
            on_duplicate_key: Option<IdentityFailurePolicy>,
            #[serde(flatten)]
            extra: BTreeMap<String, serde_json::Value>,
        }

        let raw = RawPathConfigEntry::deserialize(deserializer)?;
        let mut row_identity = raw.row_identity;
        if !raw.columns.is_empty() || raw.on_null_key.is_some() || raw.on_duplicate_key.is_some() {
            let mut identity = row_identity.unwrap_or_default();
            if !raw.columns.is_empty() && identity.columns.is_none() {
                identity.columns = Some(raw.columns);
            }
            if let Some(policy) = raw.on_null_key {
                identity.on_null_key.get_or_insert(policy);
            }
            if let Some(policy) = raw.on_duplicate_key {
                identity.on_duplicate_key.get_or_insert(policy);
            }
            row_identity = Some(identity);
        }

        Ok(Self {
            match_: raw.match_,
            content_type: raw.content_type,
            rule: raw.rule,
            dialect: raw.dialect,
            shape: raw.shape,
            records_path: raw.records_path,
            row_identity,
            node_identity: raw.node_identity,
            unknown_fields: raw.extra.into_keys().collect(),
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CorrespondenceConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand_renamed_unchanged_collections: Option<bool>,
    /// Byte threshold above which stdlib tabular rules skip in-memory
    /// `tabular_v1` materialization and use the bounded streaming keyed-writer
    /// path instead. `None` uses the stdlib default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_tabular_threshold_bytes: Option<u64>,
    /// Maximum decompressed size of a single gzip stream, in bytes. `None` uses
    /// the stdlib default. Raise this for legitimately large `.gz` payloads;
    /// the cap exists only as a decompression-bomb bound, so any value over a
    /// bundle's real size is safe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_gzip_bytes: Option<u64>,
    /// Maximum decompressed size of a single archive entry (one member of a
    /// `.zip`/`.tar`/`.tgz`), in bytes. `None` uses the stdlib default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_archive_entry_bytes: Option<u64>,
    /// Maximum total decompressed size of a whole archive (sum over all
    /// extracted entries), in bytes. `None` uses the stdlib default. This is the
    /// cap a real multi-GB government bundle is most likely to hit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_archive_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileIdentityConfig {
    #[serde(default)]
    pub correspondences: Vec<FileCorrespondenceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCorrespondenceRule {
    pub name: String,
    #[serde(default)]
    pub left: FileSelector,
    #[serde(default)]
    pub right: FileSelector,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_path: Option<String>,
    #[serde(default)]
    pub cardinality: Cardinality,
    #[serde(default)]
    pub on_null_key: IdentityFailurePolicy,
    #[serde(default)]
    pub on_duplicate_key: IdentityFailurePolicy,
    #[serde(default)]
    pub report_path_change: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_regex: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cardinality {
    #[default]
    OneToOne,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityFailurePolicy {
    #[default]
    Diagnostic,
    Error,
    Ignore,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TableConfig {
    #[serde(default)]
    pub defaults: TableDefaults,
    #[serde(default)]
    pub entries: Vec<TableEntry>,
}

impl<'de> Deserialize<'de> for TableConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[allow(clippy::large_enum_variant)]
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Entries(Vec<TableEntry>),
            Full {
                #[serde(default)]
                defaults: TableDefaults,
                #[serde(default)]
                entries: Vec<TableEntry>,
            },
        }

        match Repr::deserialize(deserializer)? {
            Repr::Entries(entries) => Ok(Self {
                defaults: TableDefaults::default(),
                entries,
            }),
            Repr::Full { defaults, entries } => Ok(Self { defaults, entries }),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableDefaults {
    #[serde(default)]
    pub parse: TabularParseConfig,
    #[serde(default)]
    pub row_identity: RowIdentity,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TableEntry {
    #[serde(default, rename = "match")]
    pub match_: TableSelector,
    #[serde(default)]
    pub parse: TabularParseConfig,
    #[serde(default)]
    pub row_identity: RowIdentityPatch,
}

impl<'de> Deserialize<'de> for TableEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Default, Deserialize)]
        struct RawTableEntry {
            #[serde(default, rename = "match")]
            match_: TableSelector,
            #[serde(default)]
            parse: TabularParseConfig,
            #[serde(default)]
            row_identity: RowIdentityPatch,
            #[serde(default)]
            logical_name: Option<String>,
            #[serde(default)]
            path: Option<String>,
            #[serde(default)]
            path_regex: Option<String>,
            #[serde(default)]
            columns: Vec<String>,
            #[serde(default)]
            on_null_key: Option<IdentityFailurePolicy>,
            #[serde(default)]
            on_duplicate_key: Option<IdentityFailurePolicy>,
        }

        let raw = RawTableEntry::deserialize(deserializer)?;
        let mut match_ = raw.match_;
        if match_.logical_name.is_none() {
            match_.logical_name = raw.logical_name;
        }
        if match_.source.is_none() && (raw.path.is_some() || raw.path_regex.is_some()) {
            match_.source = Some(FileSelector {
                path: raw.path,
                path_regex: raw.path_regex,
            });
        }

        let mut row_identity = raw.row_identity;
        if row_identity.columns.is_none() && !raw.columns.is_empty() {
            row_identity.columns = Some(raw.columns);
        }
        if let Some(policy) = raw.on_null_key {
            row_identity.on_null_key.get_or_insert(policy);
        }
        if let Some(policy) = raw.on_duplicate_key {
            row_identity.on_duplicate_key.get_or_insert(policy);
        }

        Ok(Self {
            match_,
            parse: raw.parse,
            row_identity,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FileSelector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TabularParseConfig {
    #[serde(default = "default_header")]
    pub header: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<CsvDialectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_lines: Option<usize>,
    /// JSON record collection path for document formats that need to expose a
    /// nested array as the tabular record stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub records_path: Option<String>,
}

impl Default for TabularParseConfig {
    fn default() -> Self {
        Self {
            header: true,
            delimiter: None,
            dialect: None,
            header_line: None,
            skip_lines: None,
            records_path: None,
        }
    }
}

fn default_header() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CsvDialectConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newline: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TabularShapeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_lines: Option<usize>,
}

impl TabularShapeConfig {
    pub fn apply_to_parse_config(&self, parse: &mut TabularParseConfig) {
        if let Some(has_header) = self.has_header {
            parse.header = has_header;
        }
        if let Some(header_line) = self.header_line {
            parse.header_line = Some(header_line);
        }
        if let Some(skip_lines) = self.skip_lines {
            parse.skip_lines = Some(skip_lines);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RowIdentity {
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub by_position: Vec<usize>,
    #[serde(default)]
    pub cardinality: Cardinality,
    #[serde(default)]
    pub on_null_key: IdentityFailurePolicy,
    #[serde(default)]
    pub on_duplicate_key: IdentityFailurePolicy,
}

/// Presence-preserving row-identity overrides for a selected path or table.
///
/// Runtime identities and dataset defaults use [`RowIdentity`]. Entry-level
/// configuration uses this patch type so an explicitly configured default
/// enum value is distinguishable from an omitted field. `columns` and
/// `by_position` are alternate key selectors; when both are present,
/// `columns` takes precedence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowIdentityPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_position: Option<Vec<usize>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<Cardinality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_null_key: Option<IdentityFailurePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_duplicate_key: Option<IdentityFailurePolicy>,
}

impl RowIdentityPatch {
    /// Apply this entry-level override to a concrete inherited identity.
    pub fn apply_to(&self, identity: &mut RowIdentity) {
        if let Some(columns) = &self.columns {
            identity.columns = columns.clone();
            identity.by_position.clear();
        } else if let Some(by_position) = &self.by_position {
            identity.by_position = by_position.clone();
            identity.columns.clear();
        }
        if let Some(cardinality) = self.cardinality {
            identity.cardinality = cardinality;
        }
        if let Some(policy) = self.on_null_key {
            identity.on_null_key = policy;
        }
        if let Some(policy) = self.on_duplicate_key {
            identity.on_duplicate_key = policy;
        }
    }

    pub fn has_key_selector(&self) -> bool {
        self.columns
            .as_ref()
            .is_some_and(|columns| !columns.is_empty())
            || self
                .by_position
                .as_ref()
                .is_some_and(|positions| !positions.is_empty())
    }
}

/// A pair of tabular data (left/right sides of a comparison).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabularDataPair {
    pub left: Option<TabularData>,
    pub right: Option<TabularData>,
}

impl TabularDataPair {
    /// Build a `TabularDataPair` from [`tabular_v1`] artifacts on a node.
    ///
    /// Returns `None` if neither left nor right artifact is present.
    /// This is the standard way for rules and extractors to obtain
    /// tabular data without knowing the source format.
    pub fn from_artifacts(
        node: &crate::ir::DiffNode,
        data: &dyn crate::traits::DataAccess,
    ) -> Option<Self> {
        let fmt = tabular_v1();
        let left = node
            .artifacts
            .iter()
            .find(|a| a.format == fmt && a.subject == ArtifactSubject::Left)
            .and_then(|desc| data.get_artifact(desc).ok()?)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let right = node
            .artifacts
            .iter()
            .find(|a| a.format == fmt && a.subject == ArtifactSubject::Right)
            .and_then(|desc| data.get_artifact(desc).ok()?)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        if left.is_none() && right.is_none() {
            return None;
        }
        Some(Self { left, right })
    }
}

// ── Tabular extraction ──────────────────────────────────────────────

/// Shared extraction logic for tabular data.
///
/// Given a `TabularDataPair` and an aspect name, produces the
/// corresponding `ExtractResult`. This is format-neutral — any
/// writer or compatibility plugin that works with tabular artifacts can
/// delegate extraction here.
pub fn tabular_extract(
    pair: &TabularDataPair,
    _node: &DiffNode,
    aspect: &str,
) -> Option<ExtractResult> {
    match aspect {
        "rows_added" => {
            let right = pair.right.as_ref()?;
            let left_len = pair.left.as_ref().map_or(0, |l| l.rows.len());
            if left_len >= right.rows.len() {
                return Some(ExtractResult::Text("No rows added.\n".into()));
            }
            let added = TabularData::new(right.headers.clone(), right.rows[left_len..].to_vec());
            Some(ExtractResult::Text(added.to_csv()))
        }
        "rows_removed" => {
            let left = pair.left.as_ref()?;
            let right_len = pair.right.as_ref().map_or(0, |r| r.rows.len());
            if right_len >= left.rows.len() {
                return Some(ExtractResult::Text("No rows removed.\n".into()));
            }
            let removed = TabularData::new(left.headers.clone(), left.rows[right_len..].to_vec());
            Some(ExtractResult::Text(removed.to_csv()))
        }
        "cells_changed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let common_cols = tabular_columns_in_common(left, right);
            let min_rows = left.rows.len().min(right.rows.len());

            let mut out = String::from("row,column,old_value,new_value\n");
            for i in 0..min_rows {
                for col in &common_cols {
                    let li = left.column_index(col)?;
                    let ri = right.column_index(col)?;
                    let lv = left.rows[i].get(li).unwrap_or(&NULL_VALUE);
                    let rv = right.rows[i].get(ri).unwrap_or(&NULL_VALUE);
                    if lv != rv {
                        out.push_str(&format!("{i},{col},{},{}\n", lv.as_text(), rv.as_text()));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_added" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let left_set: std::collections::BTreeSet<&str> =
                left.headers.iter().map(|s| s.as_str()).collect();
            let added: Vec<&str> = right
                .headers
                .iter()
                .filter(|h| !left_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if added.is_empty() {
                return Some(ExtractResult::Text("No columns added.\n".into()));
            }
            let mut out = String::new();
            for col in &added {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = right.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {}\n", val.as_text()));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "columns_removed" => {
            let left = pair.left.as_ref()?;
            let right = pair.right.as_ref()?;
            let right_set: std::collections::BTreeSet<&str> =
                right.headers.iter().map(|s| s.as_str()).collect();
            let removed: Vec<&str> = left
                .headers
                .iter()
                .filter(|h| !right_set.contains(h.as_str()))
                .map(|h| h.as_str())
                .collect();
            if removed.is_empty() {
                return Some(ExtractResult::Text("No columns removed.\n".into()));
            }
            let mut out = String::new();
            for col in &removed {
                out.push_str(&format!("{col}\n"));
                if let Some(vals) = left.column_values(col) {
                    for val in vals {
                        out.push_str(&format!("  {}\n", val.as_text()));
                    }
                }
            }
            Some(ExtractResult::Text(out))
        }
        "content" | "full" => {
            let mut out = String::new();
            if let Some(left) = &pair.left {
                out.push_str("--- left\n");
                out.push_str(&left.to_csv());
            }
            if let Some(right) = &pair.right {
                out.push_str("+++ right\n");
                out.push_str(&right.to_csv());
            }
            Some(ExtractResult::Text(out))
        }
        _ => None,
    }
}

fn tabular_columns_in_common(left: &TabularData, right: &TabularData) -> Vec<String> {
    let left_set: std::collections::BTreeSet<&str> =
        left.headers.iter().map(|s| s.as_str()).collect();
    right
        .headers
        .iter()
        .filter(|h| left_set.contains(h.as_str()))
        .cloned()
        .collect()
}

// ── Item types ──────────────────────────────────────────────────────

/// Metadata-only view of one side of a comparison. Carries logical identity
/// and content metadata but NOT a filesystem path — data access goes through
/// `DataAccess`.
///
/// # Metadata invariants
///
/// `content_hash`, `size`, and `media_type` are **opportunistic hints**.
/// Producers (expand rules like directory/zip, or data backends)
/// populate them when doing so is cheap — typically as a byproduct of work
/// they were already performing. Consumers **must not assume presence**, but
/// **may trust presence**: when a field is set, the value accurately reflects
/// the current bytes. Use [`ItemRef::resolve_hash`] / [`ItemRef::resolve_size`]
/// to obtain a value with a transparent fall-back read.
///
/// This keeps fast paths (directory-only listings, short-circuit identical
/// detection) cheap while letting consumers that need a value — most notably
/// the move detector, which correlates leaves across container boundaries —
/// hydrate on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ItemRef {
    /// User-meaningful location within a snapshot. `/>` marks a
    /// decompose boundary; a literal segment beginning with `>` is escaped
    /// as `\>`.
    pub logical_path: String,
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Optional projection metadata supplied by rule packs while they still
    /// know the vocabulary. Core carries this through but does not interpret
    /// file names, media types, or plugin-specific tags.
    #[serde(default, skip_serializing_if = "crate::projection_hint_is_default")]
    pub projection_hint: crate::ProjectionHint,
    /// Optional stdlib-resolved tabular parse hints carried from dataset config
    /// to whichever tabular parser eventually claims this item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabular_parse: Option<TabularParseConfig>,
    /// Opaque identifier used by DataAccess implementations to locate data.
    /// Plugin authors should not create or interpret this value directly.
    #[serde(default)]
    pub handle: String,
}

impl ItemRef {
    pub fn extension(&self) -> Option<String> {
        std::path::Path::new(&self.logical_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
    }

    /// Return the item's BLAKE3 content hash, computing it from bytes if
    /// not already cached on this `ItemRef`. Never valid for directories.
    pub fn resolve_hash(&self, data: &dyn crate::DataAccess) -> crate::BinocResult<String> {
        if let Some(hash) = &self.content_hash {
            return Ok(hash.clone());
        }
        let mut reader = data.open_read(self)?;
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut reader, &mut hasher)?;
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Return the item's byte length, reading from the backend if not already
    /// cached on this `ItemRef`. Never valid for directories.
    pub fn resolve_size(&self, data: &dyn crate::DataAccess) -> crate::BinocResult<u64> {
        if let Some(size) = self.size {
            return Ok(size);
        }
        let bytes = data.read_bytes(self)?;
        Ok(bytes.len() as u64)
    }
}

/// A pair of items to compare. Either side may be None (add/remove).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ItemPair {
    pub left: Option<ItemRef>,
    pub right: Option<ItemRef>,
}

impl ItemPair {
    pub fn both(left: ItemRef, right: ItemRef) -> Self {
        Self {
            left: Some(left),
            right: Some(right),
        }
    }

    pub fn added(right: ItemRef) -> Self {
        Self {
            left: None,
            right: Some(right),
        }
    }

    pub fn removed(left: ItemRef) -> Self {
        Self {
            left: Some(left),
            right: None,
        }
    }

    pub fn logical_path(&self) -> &str {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .map(|i| i.logical_path.as_str())
            .unwrap_or("")
    }

    pub fn extension(&self) -> Option<String> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.extension())
    }

    pub fn media_type(&self) -> Option<&str> {
        self.right
            .as_ref()
            .or(self.left.as_ref())
            .and_then(|i| i.media_type.as_deref())
    }

    pub fn is_dir(&self) -> bool {
        self.right.as_ref().is_some_and(|i| i.is_dir)
            || self.left.as_ref().is_some_and(|i| i.is_dir)
    }

    pub fn matching_content_hash(&self) -> Option<&str> {
        match (&self.left, &self.right) {
            (Some(l), Some(r)) => match (&l.content_hash, &r.content_hash) {
                (Some(hl), Some(hr)) if hl == hr => Some(hl.as_str()),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Result of an extract (on-demand detail retrieval) operation.
pub enum ExtractResult {
    Text(String),
    Binary(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare_item(logical: &str, is_dir: bool) -> ItemRef {
        ItemRef {
            logical_path: logical.into(),
            is_dir,
            content_hash: None,
            size: None,
            media_type: None,
            projection_hint: Default::default(),
            tabular_parse: None,
            handle: String::new(),
        }
    }

    #[test]
    fn item_ref_extension() {
        let item = bare_item("data.csv", false);
        assert_eq!(item.extension(), Some(".csv".into()));
    }

    #[test]
    fn item_ref_extension_none() {
        let item = bare_item("Makefile", false);
        assert_eq!(item.extension(), None);
    }

    #[test]
    fn item_pair_logical_path_prefers_right() {
        let left = bare_item("left.txt", false);
        let right = bare_item("right.txt", false);
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.logical_path(), "right.txt");
    }

    #[test]
    fn item_pair_logical_path_falls_back_to_left() {
        let left = bare_item("only.txt", false);
        let pair = ItemPair::removed(left);
        assert_eq!(pair.logical_path(), "only.txt");
    }

    #[test]
    fn item_pair_is_dir() {
        let dir = bare_item("sub", true);
        let pair = ItemPair::added(dir);
        assert!(pair.is_dir());
    }

    #[test]
    fn item_pair_matching_hash() {
        let mut left = bare_item("f", false);
        left.content_hash = Some("abc".into());
        let mut right = bare_item("f", false);
        right.content_hash = Some("abc".into());
        let pair = ItemPair::both(left, right);
        assert_eq!(pair.matching_content_hash(), Some("abc"));
    }

    #[test]
    fn row_identity_patch_deserialization_preserves_explicit_default_policy() {
        let semantics: DatasetSemanticsV1 = serde_json::from_value(serde_json::json!({
            "paths": [{
                "match": "data.csv",
                "row_identity": {
                    "columns": ["id"],
                    "on_null_key": "diagnostic"
                }
            }]
        }))
        .expect("dataset semantics");

        let patch = semantics.paths[0]
            .row_identity
            .as_ref()
            .expect("row identity patch");
        assert_eq!(
            patch.columns.as_deref(),
            Some([String::from("id")].as_slice())
        );
        assert_eq!(patch.on_null_key, Some(IdentityFailurePolicy::Diagnostic));
        assert_eq!(patch.on_duplicate_key, None);

        let serialized = serde_json::to_value(&semantics.paths[0]).expect("serialize path entry");
        assert_eq!(serialized["row_identity"]["on_null_key"], "diagnostic");
        assert!(serialized["row_identity"].get("on_duplicate_key").is_none());
    }

    #[test]
    fn row_identity_patch_replaces_inherited_key_selector() {
        let mut identity = RowIdentity {
            columns: vec!["id".into()],
            on_null_key: IdentityFailurePolicy::Error,
            ..RowIdentity::default()
        };
        RowIdentityPatch {
            by_position: Some(vec![2]),
            on_null_key: Some(IdentityFailurePolicy::Diagnostic),
            ..RowIdentityPatch::default()
        }
        .apply_to(&mut identity);

        assert!(identity.columns.is_empty());
        assert_eq!(identity.by_position, vec![2]);
        assert_eq!(identity.on_null_key, IdentityFailurePolicy::Diagnostic);

        RowIdentityPatch {
            columns: Some(vec!["email".into()]),
            ..RowIdentityPatch::default()
        }
        .apply_to(&mut identity);
        assert_eq!(identity.columns, vec!["email"]);
        assert!(identity.by_position.is_empty());
    }
}
