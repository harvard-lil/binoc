use std::borrow::Cow;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use binoc_sdk::*;
use encoding_rs::{Encoding, UTF_8, WINDOWS_1252};
use encoding_rs_io::DecodeReaderBytesBuilder;

/// Thin CSV comparator: parses CSV into [`TabularData`], publishes
/// [`tabular_v1`] artifacts, and checks logical identity. All semantic
/// analysis (column changes, row changes, cell diffs) is handled by the
/// [`TabularAnalyzer`](crate::transformers::tabular_analyzer::TabularAnalyzer)
/// transformer, which operates on the published artifacts and is
/// source-format-agnostic.
pub struct CsvComparator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CsvParseOptions {
    delimiter: u8,
}

impl CsvParseOptions {
    fn for_item(item: &ItemRef) -> Self {
        // TODO(#48/#51): move comparator parse options into the dataset-config
        // surface once per-target comparator config lands.
        let delimiter = match item.extension().as_deref() {
            Some(".tsv") => b'\t',
            _ => b',',
        };
        Self { delimiter }
    }
}

type DynRead = Box<dyn Read + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Utf8SampleStats {
    valid_multibyte_sequences: usize,
    valid_multibyte_bytes: usize,
    invalid_bytes: usize,
}

fn utf8_sample_stats(sample: &[u8]) -> Utf8SampleStats {
    let mut stats = Utf8SampleStats {
        valid_multibyte_sequences: 0,
        valid_multibyte_bytes: 0,
        invalid_bytes: 0,
    };
    let mut i = 0;
    while i < sample.len() {
        let byte = sample[i];
        if byte <= 0x7F {
            i += 1;
            continue;
        }

        let width = if (0xC2..=0xDF).contains(&byte) {
            2
        } else if (0xE0..=0xEF).contains(&byte) {
            3
        } else if (0xF0..=0xF4).contains(&byte) {
            4
        } else {
            stats.invalid_bytes += 1;
            i += 1;
            continue;
        };

        if i + width > sample.len() {
            stats.invalid_bytes += sample.len() - i;
            break;
        }

        let seq = &sample[i..i + width];
        let valid = match width {
            2 => (0x80..=0xBF).contains(&seq[1]),
            3 => match seq[0] {
                0xE0 => (0xA0..=0xBF).contains(&seq[1]) && (0x80..=0xBF).contains(&seq[2]),
                0xED => (0x80..=0x9F).contains(&seq[1]) && (0x80..=0xBF).contains(&seq[2]),
                _ => (0x80..=0xBF).contains(&seq[1]) && (0x80..=0xBF).contains(&seq[2]),
            },
            4 => match seq[0] {
                0xF0 => {
                    (0x90..=0xBF).contains(&seq[1])
                        && (0x80..=0xBF).contains(&seq[2])
                        && (0x80..=0xBF).contains(&seq[3])
                }
                0xF4 => {
                    (0x80..=0x8F).contains(&seq[1])
                        && (0x80..=0xBF).contains(&seq[2])
                        && (0x80..=0xBF).contains(&seq[3])
                }
                _ => {
                    (0x80..=0xBF).contains(&seq[1])
                        && (0x80..=0xBF).contains(&seq[2])
                        && (0x80..=0xBF).contains(&seq[3])
                }
            },
            _ => unreachable!(),
        };

        if valid {
            stats.valid_multibyte_sequences += 1;
            stats.valid_multibyte_bytes += width;
            i += width;
        } else {
            stats.invalid_bytes += 1;
            i += 1;
        }
    }

    stats
}

fn should_prefer_utf8_despite_invalid_bytes(sample: &[u8]) -> bool {
    let stats = utf8_sample_stats(sample);
    stats.valid_multibyte_sequences > 0 && stats.valid_multibyte_bytes >= stats.invalid_bytes * 2
}

fn sniff_encoding(reader: &mut BufReader<DynRead>) -> BinocResult<Option<&'static Encoding>> {
    let sample = reader.fill_buf().map_err(BinocError::Io)?;
    if sample.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok(None);
    }
    if std::str::from_utf8(sample).is_ok() {
        return Ok(Some(UTF_8));
    }
    if should_prefer_utf8_despite_invalid_bytes(sample) {
        return Ok(Some(UTF_8));
    }
    Ok(Some(WINDOWS_1252))
}

pub(crate) fn csv_reader_from_read(
    reader: DynRead,
    delimiter: u8,
) -> BinocResult<csv::Reader<DynRead>> {
    let mut buffered = BufReader::new(reader);
    let encoding = sniff_encoding(&mut buffered)?;
    let decoded: DynRead = Box::new(
        DecodeReaderBytesBuilder::new()
            .encoding(encoding)
            .build(buffered),
    );

    Ok(csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(decoded))
}

pub(crate) fn lossy_field(record: &csv::ByteRecord, index: usize) -> Cow<'_, str> {
    String::from_utf8_lossy(record.get(index).unwrap_or(b""))
}

pub(crate) fn lossy_record(record: &csv::ByteRecord) -> Vec<String> {
    record
        .iter()
        .map(|field| String::from_utf8_lossy(field).into_owned())
        .collect()
}

pub(crate) fn csv_headers<R: Read>(rdr: &mut csv::Reader<R>) -> BinocResult<Vec<String>> {
    let headers = rdr
        .byte_headers()
        .map_err(|e| BinocError::Csv(e.to_string()))?;
    Ok(lossy_record(headers))
}

fn parse_csv(path: &Path, options: CsvParseOptions) -> BinocResult<TabularData> {
    let file = std::fs::File::open(path).map_err(BinocError::Io)?;
    let mut rdr = csv_reader_from_read(Box::new(file), options.delimiter)?;
    let headers = csv_headers(&mut rdr)?;

    let mut rows = Vec::new();
    let mut record = csv::ByteRecord::new();
    while rdr
        .read_byte_record(&mut record)
        .map_err(|e| BinocError::Csv(e.to_string()))?
    {
        rows.push(lossy_record(&record));
    }

    Ok(TabularData { headers, rows })
}

fn parse_csv_via_data(item: &ItemRef, data: &dyn DataAccess) -> BinocResult<TabularData> {
    let path = data.local_path(item)?;
    parse_csv(&path, CsvParseOptions::for_item(item))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(logical_path: &str) -> ItemRef {
        ItemRef {
            logical_path: logical_path.into(),
            is_dir: false,
            content_hash: None,
            size: None,
            media_type: None,
            handle: String::new(),
        }
    }

    #[test]
    fn parse_options_use_tab_for_tsv() {
        assert_eq!(
            CsvParseOptions::for_item(&item("table.tsv")),
            CsvParseOptions { delimiter: b'\t' }
        );
    }

    #[test]
    fn parse_options_default_to_comma() {
        assert_eq!(
            CsvParseOptions::for_item(&item("table.csv")),
            CsvParseOptions { delimiter: b',' }
        );
        assert_eq!(
            CsvParseOptions::for_item(&item("table.unknown")),
            CsvParseOptions { delimiter: b',' }
        );
    }
    #[test]
    fn parse_csv_strips_utf8_bom_from_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bom.csv");
        std::fs::write(&path, b"\xEF\xBB\xBFid,name\n1,Alice\n").unwrap();

        let parsed = parse_csv(&path, CsvParseOptions { delimiter: b',' }).unwrap();

        assert_eq!(parsed.headers, vec!["id", "name"]);
        assert_eq!(
            parsed.rows,
            vec![vec!["1".to_string(), "Alice".to_string()]]
        );
    }

    #[test]
    fn parse_csv_transcodes_windows_1252() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cp1252.csv");
        std::fs::write(&path, b"id,name\n1,Jos\xe9\n").unwrap();

        let parsed = parse_csv(&path, CsvParseOptions { delimiter: b',' }).unwrap();

        assert_eq!(parsed.rows[0][1], "José");
    }

    #[test]
    fn parse_csv_tolerates_invalid_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("invalid.csv");
        std::fs::write(&path, b"id,name\n1,Alice\n2,bad\xffvalue\n3,Carol\n").unwrap();

        let parsed = parse_csv(&path, CsvParseOptions { delimiter: b',' }).unwrap();

        assert_eq!(parsed.rows.len(), 3);
        assert_eq!(parsed.rows[0][1], "Alice");
        assert_eq!(parsed.rows[2][1], "Carol");
    }

    #[test]
    fn parse_csv_prefers_utf8_when_corruption_follows_valid_utf8() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mostly-utf8.csv");
        std::fs::write(&path, b"id,name\n1,Jos\xc3\xa9\n2,bad\xffvalue\n").unwrap();

        let parsed = parse_csv(&path, CsvParseOptions { delimiter: b',' }).unwrap();

        assert_eq!(parsed.rows[0][1], "José");
        assert_eq!(parsed.rows[1][1], "bad�value");
    }

    #[test]
    fn utf8_sample_stats_counts_valid_and_invalid_non_ascii_bytes() {
        let stats = utf8_sample_stats(b"Jos\xc3\xa9\xff");
        assert_eq!(
            stats,
            Utf8SampleStats {
                valid_multibyte_sequences: 1,
                valid_multibyte_bytes: 2,
                invalid_bytes: 1,
            }
        );
    }
}
