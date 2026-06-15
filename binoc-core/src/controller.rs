use std::path::Path;
use std::sync::Arc;

use binoc_sdk::*;

use crate::correspondence::driver::DescriptionCost;
use crate::correspondence::{
    driver as correspondence_driver, CorrespondenceEngineConfig, RunTrace,
};
use crate::data_access::LocalDataAccess;

const MAX_CHANGESET_DIAGNOSTICS: usize = 16;

/// The core engine: builds side trees, saturates correspondence rules, and
/// projects the resulting links to the public changeset tree.
///
/// Type-ignorant by construction: format and dataset semantics live in rule
/// packs, not in the controller.
pub struct Controller {
    correspondence_engine: CorrespondenceEngineConfig,
    dataset_config: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct DiffMetrics {
    pub description_cost: DescriptionCost,
}

#[derive(Debug, Clone)]
pub struct DiffRun {
    pub changeset: Changeset,
    pub metrics: DiffMetrics,
}

impl Controller {
    pub fn new(correspondence_engine: CorrespondenceEngineConfig) -> Self {
        Self {
            correspondence_engine,
            dataset_config: serde_json::Value::Null,
        }
    }

    /// Attach dataset-level semantic configuration. The controller does not
    /// deserialize this value; it is exposed to rule packs through the
    /// correspondence dataset configurator.
    pub fn with_dataset_config(mut self, config: serde_json::Value) -> Self {
        self.dataset_config = config;
        self
    }

    /// Diff two snapshots and produce a changeset.
    pub fn diff(&self, from_path: &str, to_path: &str) -> BinocResult<Changeset> {
        Ok(self.diff_with_metrics(from_path, to_path)?.changeset)
    }

    /// Diff two snapshots and produce a changeset with harness-oriented run
    /// metrics. Metrics are derived from the correspondence run before the
    /// public changeset strips transient fields.
    pub fn diff_with_metrics(&self, from_path: &str, to_path: &str) -> BinocResult<DiffRun> {
        let data = Arc::new(LocalDataAccess::new_for_diff(
            Path::new(from_path),
            Path::new(to_path),
        )?);
        let pair = Self::make_root_pair(from_path, to_path, &data)?;
        let left = pair
            .left
            .ok_or_else(|| BinocError::Other("correspondence root has no left item".into()))?;
        let right = pair
            .right
            .ok_or_else(|| BinocError::Other("correspondence root has no right item".into()))?;
        let mut config = self.correspondence_engine.clone();
        let mut setup_diagnostics = Vec::new();
        if let Some(configurator) = config.dataset_configurator.clone() {
            setup_diagnostics.extend(configurator.configure(
                &mut config,
                &self.dataset_config,
                &left,
                &right,
                data.as_ref(),
            )?);
        }
        let run = correspondence_driver::run(&config, left, right, data.as_ref())?;
        let description_cost = run.description_cost();
        let mut changeset = run.project().to_changeset(from_path, to_path);
        changeset.diagnostics.extend(setup_diagnostics);
        changeset.diagnostics.extend(run.diagnostics);
        changeset.claims.extend(run.claims);
        changeset.hoist_node_diagnostics();
        changeset.dedupe_and_cap_diagnostics(MAX_CHANGESET_DIAGNOSTICS);
        changeset.strip_transient();
        Ok(DiffRun {
            changeset,
            metrics: DiffMetrics { description_cost },
        })
    }

    /// Diff two snapshots and produce a changeset together with a full replay
    /// [`RunTrace`] of the correspondence run (every expand/parse/link/write/
    /// compaction step). Runs serially for deterministic step ordering; use for
    /// debugging and visualization rather than throughput.
    pub fn diff_with_trace(
        &self,
        from_path: &str,
        to_path: &str,
    ) -> BinocResult<(Changeset, RunTrace)> {
        let data = Arc::new(LocalDataAccess::new_for_diff(
            Path::new(from_path),
            Path::new(to_path),
        )?);
        let pair = Self::make_root_pair(from_path, to_path, &data)?;
        let left = pair
            .left
            .ok_or_else(|| BinocError::Other("correspondence root has no left item".into()))?;
        let right = pair
            .right
            .ok_or_else(|| BinocError::Other("correspondence root has no right item".into()))?;
        let mut config = self.correspondence_engine.clone();
        let mut setup_diagnostics = Vec::new();
        if let Some(configurator) = config.dataset_configurator.clone() {
            setup_diagnostics.extend(configurator.configure(
                &mut config,
                &self.dataset_config,
                &left,
                &right,
                data.as_ref(),
            )?);
        }
        let (run, mut trace) =
            correspondence_driver::run_traced(&config, left, right, data.as_ref())?;
        let mut changeset = run.project().to_changeset(from_path, to_path);
        changeset.diagnostics.extend(setup_diagnostics);
        changeset.diagnostics.extend(run.diagnostics);
        changeset.claims.extend(run.claims);
        changeset.hoist_node_diagnostics();
        changeset.dedupe_and_cap_diagnostics(MAX_CHANGESET_DIAGNOSTICS);
        changeset.strip_transient();
        trace.from_snapshot = from_path.to_string();
        trace.to_snapshot = to_path.to_string();
        Ok((changeset, trace))
    }

    /// Diff an ordered snapshot sequence and produce one changeset per
    /// consecutive pair.
    pub fn diff_many<S>(&self, snapshots: &[S]) -> BinocResult<Vec<Changeset>>
    where
        S: AsRef<str>,
    {
        if snapshots.len() < 2 {
            return Err(BinocError::Config(
                "diff_many requires at least two snapshots".into(),
            ));
        }

        snapshots
            .windows(2)
            .map(|pair| self.diff(pair[0].as_ref(), pair[1].as_ref()))
            .collect()
    }

    /// Build the root `ItemPair` for a diff.
    ///
    /// Directories get `logical_path = ""` (their children build relative
    /// paths). Files get the filename from `to_path` (or `from_path` as
    /// fallback) so extension-based rules can match the root item.
    fn make_root_pair(
        from_path: &str,
        to_path: &str,
        data: &Arc<LocalDataAccess>,
    ) -> BinocResult<ItemPair> {
        let from = Path::new(from_path);
        let to = Path::new(to_path);

        let logical = if to.is_dir() && from.is_dir() {
            String::new()
        } else {
            Self::filename_or_empty(to)
                .or_else(|| Self::filename_or_empty(from))
                .map(escape_segment)
                .unwrap_or_default()
        };

        let left = data.register_local(from, &logical)?;
        let right = data.register_local(to, &logical)?;
        Ok(ItemPair::both(left, right))
    }

    fn filename_or_empty(path: &Path) -> Option<&str> {
        path.file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
    }

    /// Extract data from a specific node in a changeset.
    ///
    /// Extract reruns the correspondence engine and asks the writer that owns
    /// the projected link for the requested aspect.
    pub fn extract(
        &self,
        changeset: &Changeset,
        node_path: &str,
        aspect: &str,
        snapshot_a: &str,
        snapshot_b: &str,
    ) -> BinocResult<ExtractResult> {
        let root = changeset
            .root
            .as_ref()
            .ok_or_else(|| BinocError::Extract("changeset has no root".into()))?;
        let target = Self::find_node(root, node_path)
            .ok_or_else(|| BinocError::Extract(format!("node not found: {node_path}")))?;

        let data = Arc::new(LocalDataAccess::new_for_diff(
            Path::new(snapshot_a),
            Path::new(snapshot_b),
        )?);
        let pair = Self::make_root_pair(snapshot_a, snapshot_b, &data)?;
        let left = pair
            .left
            .ok_or_else(|| BinocError::Other("correspondence root has no left item".into()))?;
        let right = pair
            .right
            .ok_or_else(|| BinocError::Other("correspondence root has no right item".into()))?;
        let mut config = self.correspondence_engine.clone();
        if let Some(configurator) = config.dataset_configurator.clone() {
            configurator.configure(
                &mut config,
                &self.dataset_config,
                &left,
                &right,
                data.as_ref(),
            )?;
        }
        let run = correspondence_driver::run(&config, left, right, data.as_ref())?;
        let projection = run.project();
        let candidates = projection.find(node_path);
        let line = candidates
            .iter()
            .copied()
            .find(|line| line.sources == target.sources)
            .or_else(|| candidates.first().copied())
            .ok_or_else(|| {
                BinocError::Extract(format!(
                    "node '{node_path}' was not reproduced by correspondence projection"
                ))
            })?;
        run.extract_line(&config, line, aspect, data.as_ref())
    }

    fn find_node<'a>(node: &'a DiffNode, target_path: &str) -> Option<&'a DiffNode> {
        if node.path == target_path {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = Self::find_node(child, target_path) {
                return Some(found);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_diff_many_rejects_short_sequences() {
        let controller = Controller::new(CorrespondenceEngineConfig::default());
        let err = controller.diff_many::<String>(&[]).unwrap_err();
        assert!(err.to_string().contains("at least two snapshots"));
    }

    #[test]
    fn root_pair_uses_empty_logical_path_for_directory_roots() {
        let dir = tempfile::tempdir().unwrap();
        let data = Arc::new(LocalDataAccess::new_for_diff(dir.path(), dir.path()).unwrap());

        let pair = Controller::make_root_pair(
            dir.path().to_string_lossy().as_ref(),
            dir.path().to_string_lossy().as_ref(),
            &data,
        )
        .unwrap();

        assert_eq!(pair.logical_path(), "");
    }

    #[test]
    fn root_pair_uses_filename_for_file_roots() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.txt");
        let right = dir.path().join("right.txt");
        std::fs::write(&left, "a").unwrap();
        std::fs::write(&right, "b").unwrap();
        let data = Arc::new(LocalDataAccess::new_for_diff(&left, &right).unwrap());

        let pair = Controller::make_root_pair(
            left.to_string_lossy().as_ref(),
            right.to_string_lossy().as_ref(),
            &data,
        )
        .unwrap();

        assert_eq!(pair.logical_path(), "right.txt");
    }

    #[test]
    fn root_pair_escapes_leading_decompose_marker_for_file_roots() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.txt");
        let right = dir.path().join(">right.txt");
        std::fs::write(&left, "a").unwrap();
        std::fs::write(&right, "b").unwrap();
        let data = Arc::new(LocalDataAccess::new_for_diff(&left, &right).unwrap());

        let pair = Controller::make_root_pair(
            left.to_string_lossy().as_ref(),
            right.to_string_lossy().as_ref(),
            &data,
        )
        .unwrap();

        assert_eq!(pair.logical_path(), r"\>right.txt");
    }
}
