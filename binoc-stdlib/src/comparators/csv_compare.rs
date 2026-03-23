use std::io::BufReader;

use binoc_sdk::*;

/// Thin CSV comparator: parses CSV into [`TabularData`], publishes
/// [`tabular_v1`] artifacts, and checks logical identity. All semantic
/// analysis (column changes, row changes, cell diffs) is handled by the
/// [`TabularAnalyzer`](crate::transformers::tabular_analyzer::TabularAnalyzer)
/// transformer, which operates on the published artifacts and is
/// source-format-agnostic.
pub struct CsvComparator;

fn parse_csv(path: &std::path::Path) -> BinocResult<TabularData> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let reader = BufReader::new(file);
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(reader);

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| BinocError::Csv(e.to_string()))?
        .iter()
        .map(|s| s.to_string())
        .collect();

    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result.map_err(|e| BinocError::Csv(e.to_string()))?;
        rows.push(record.iter().map(|s| s.to_string()).collect());
    }

    Ok(TabularData { headers, rows })
}

fn parse_csv_via_data(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<TabularData> {
    let path = data.local_path(item)?;
    parse_csv(&path)
}

fn publish_tabular(
    data: &dyn DataAccess,
    tabular: &TabularData,
    subject: ArtifactSubject,
) -> BinocResult<ArtifactDescriptor> {
    let bytes = serde_json::to_vec(tabular)
        .map_err(|e| BinocError::Other(format!("serialize tabular artifact: {e}")))?;
    data.publish_artifact(&tabular_v1(), subject, "binoc.csv", &bytes)
}

impl Comparator for CsvComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.csv").with_extensions(vec![".csv".into(), ".tsv".into()])
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let csv_l = parse_csv_via_data(left, data)?;
                let csv_r = parse_csv_via_data(right, data)?;

                if csv_l == csv_r {
                    return Ok(CompareResult::Identical);
                }

                let left_artifact = publish_tabular(data, &csv_l, ArtifactSubject::Left)?;
                let right_artifact = publish_tabular(data, &csv_r, ArtifactSubject::Right)?;

                let node = DiffNode::new("modify", "tabular", pair.logical_path())
                    .with_artifact(left_artifact)
                    .with_artifact(right_artifact);

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                let csv = parse_csv_via_data(right, data)?;
                let artifact = publish_tabular(data, &csv, ArtifactSubject::Right)?;
                let node =
                    DiffNode::new("add", "tabular", &right.logical_path).with_artifact(artifact);
                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let csv = parse_csv_via_data(left, data)?;
                let artifact = publish_tabular(data, &csv, ArtifactSubject::Left)?;
                let node =
                    DiffNode::new("remove", "tabular", &left.logical_path).with_artifact(artifact);
                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }

    fn extract(
        &self,
        node: &DiffNode,
        aspect: &str,
        data: &dyn DataAccess,
    ) -> Option<ExtractResult> {
        let pair = TabularDataPair::from_artifacts(node, data)?;
        tabular_extract(&pair, node, aspect)
    }
}
