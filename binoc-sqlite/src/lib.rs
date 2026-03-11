mod sqlite;

use binoc_core::config::PluginRegistry;
use std::sync::Arc;

pub use sqlite::SqliteComparator;

/// Register the SQLite comparator into a Rust PluginRegistry.
pub fn register(registry: &mut PluginRegistry) {
    registry.register_comparator("binoc-sqlite.sqlite", Arc::new(SqliteComparator));
}

#[cfg(feature = "python")]
mod py {
    use pyo3::prelude::*;

    use binoc_core::traits::CompareContext;
    use binoc_core::types::{CompareResult, Item, ItemPair};

    /// Thin Python wrapper around the Rust SqliteComparator.
    ///
    /// Runs the real Rust comparator and serializes the result as JSON.
    /// The Python `binoc_sqlite.register()` function wraps this in a
    /// Python comparator class that deserializes the JSON back into
    /// `binoc.DiffNode` / `binoc.Leaf` / etc.
    #[pyclass(name = "_SqliteComparatorCore")]
    struct PySqliteComparatorCore {
        inner: super::SqliteComparator,
    }

    #[pymethods]
    impl PySqliteComparatorCore {
        #[new]
        fn new() -> Self {
            Self {
                inner: super::SqliteComparator,
            }
        }

        /// Run the Rust comparator and return a JSON string describing
        /// the result. Returns None for Identical.
        fn compare_json(
            &self,
            left_path: Option<String>,
            right_path: Option<String>,
            logical_path: String,
        ) -> PyResult<Option<String>> {
            let rust_pair = match (&left_path, &right_path) {
                (Some(l), Some(r)) => ItemPair::both(
                    Item::new(l.as_str(), &logical_path),
                    Item::new(r.as_str(), &logical_path),
                ),
                (None, Some(r)) => ItemPair::added(Item::new(r.as_str(), &logical_path)),
                (Some(l), None) => ItemPair::removed(Item::new(l.as_str(), &logical_path)),
                (None, None) => return Ok(None),
            };

            let ctx = CompareContext::new();
            use binoc_core::traits::Comparator;
            let result = self
                .inner
                .compare(&rust_pair, &ctx)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            match result {
                CompareResult::Identical => Ok(None),
                CompareResult::Leaf(node) => {
                    let json = serde_json::to_string(&node)
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok(Some(json))
                }
                CompareResult::Expand(node, _children) => {
                    let json = serde_json::to_string(&node)
                        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
                    Ok(Some(json))
                }
            }
        }
    }

    #[pymodule]
    fn _binoc_sqlite(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PySqliteComparatorCore>()?;
        Ok(())
    }
}
