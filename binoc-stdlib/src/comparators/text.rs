use binoc_sdk::*;
use similar::{ChangeTag, TextDiff};

const TEXT_EXTENSIONS: &[&str] = &[
    ".txt", ".md", ".rst", ".log", ".cfg", ".ini", ".toml", ".yaml", ".yml", ".json", ".xml",
    ".html", ".htm", ".css", ".js", ".py", ".rs", ".sh", ".sql", ".r", ".rb", ".pl", ".c", ".h",
    ".cpp", ".hpp", ".java",
];

/// Line-level diff comparator for text files.
pub struct TextComparator;

impl Comparator for TextComparator {
    fn descriptor(&self) -> ComparatorDescriptor {
        ComparatorDescriptor::new("binoc.text")
            .with_extensions(TEXT_EXTENSIONS.iter().map(|s| s.to_string()).collect())
    }

    fn compare(&self, pair: &ItemPair, data: &dyn DataAccess) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let bytes_l = data.read_bytes(left)?;
                let bytes_r = data.read_bytes(right)?;

                let text_l = match std::str::from_utf8(&bytes_l) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Ok(CompareResult::Skip),
                };
                let text_r = match std::str::from_utf8(&bytes_r) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Ok(CompareResult::Skip),
                };

                if text_l == text_r {
                    return Ok(CompareResult::Identical);
                }

                let diff = TextDiff::from_lines(&text_l, &text_r);

                let mut lines_added: u64 = 0;
                let mut lines_removed: u64 = 0;
                let mut lines_unchanged: u64 = 0;

                for change in diff.iter_all_changes() {
                    match change.tag() {
                        ChangeTag::Insert => lines_added += 1,
                        ChangeTag::Delete => lines_removed += 1,
                        ChangeTag::Equal => lines_unchanged += 1,
                    }
                }

                let summary = text_modify_summary(lines_added, lines_removed);

                let mut node = DiffNode::new("modify", "text", &right.logical_path)
                    .with_summary(summary)
                    .with_detail("lines_added", serde_json::json!(lines_added))
                    .with_detail("lines_removed", serde_json::json!(lines_removed))
                    .with_detail("lines_unchanged", serde_json::json!(lines_unchanged));

                if lines_added > 0 {
                    node.tags.insert("binoc.lines-added".into());
                }
                if lines_removed > 0 {
                    node.tags.insert("binoc.lines-removed".into());
                }
                if lines_added == 0 && lines_removed == 0 {
                    node.tags.insert("binoc.whitespace-change".into());
                }
                node.tags.insert("binoc.content-changed".into());

                Ok(CompareResult::Leaf(node))
            }
            (None, Some(right)) => {
                let bytes = data.read_bytes(right)?;
                let text = match std::str::from_utf8(&bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Ok(CompareResult::Skip),
                };
                let lines = text.lines().count() as u64;

                let node = DiffNode::new("add", "text", &right.logical_path)
                    .with_summary(format!(
                        "New file ({lines} line{})",
                        if lines == 1 { "" } else { "s" }
                    ))
                    .with_tag("binoc.content-changed")
                    .with_detail("lines", serde_json::json!(lines));

                Ok(CompareResult::Leaf(node))
            }
            (Some(left), None) => {
                let bytes = data.read_bytes(left)?;
                let text = match std::str::from_utf8(&bytes) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Ok(CompareResult::Skip),
                };
                let lines = text.lines().count() as u64;

                let node = DiffNode::new("remove", "text", &left.logical_path)
                    .with_summary(format!(
                        "File removed ({lines} line{})",
                        if lines == 1 { "" } else { "s" }
                    ))
                    .with_tag("binoc.content-changed")
                    .with_detail("lines", serde_json::json!(lines));

                Ok(CompareResult::Leaf(node))
            }
            (None, None) => Ok(CompareResult::Identical),
        }
    }
}

fn text_modify_summary(lines_added: u64, lines_removed: u64) -> String {
    match (lines_added, lines_removed) {
        (0, 0) => "Whitespace changes only".into(),
        (a, 0) => format!("{a} line{} added", if a == 1 { "" } else { "s" }),
        (0, r) => format!("{r} line{} removed", if r == 1 { "" } else { "s" }),
        (a, r) => format!(
            "{a} line{} added, {r} removed",
            if a == 1 { "" } else { "s" },
        ),
    }
}
