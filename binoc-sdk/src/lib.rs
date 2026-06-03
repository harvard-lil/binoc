pub mod data_access;
pub mod ir;
pub mod plugin_abi;
pub mod traits;
pub mod types;

#[cfg(feature = "test-support")]
pub mod test_support;

pub use data_access::LocalDataAccess;
pub use ir::{
    Annotation, Changeset, DetailBlock, DetailExample, Diagnostic, DiagnosticSeverity, DiffNode,
    ExtractHint, Segment, Side, Summary, ValuePreview,
};
pub use traits::*;
pub use types::*;

/// Re-exports used by macros. Not part of the public API.
#[doc(hidden)]
pub mod _reexport {
    pub use serde_json;
}
