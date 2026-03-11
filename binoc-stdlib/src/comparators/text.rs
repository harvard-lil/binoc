use binoc_core::ir::DiffNode;
use binoc_core::traits::*;
use binoc_core::types::*;
use similar::{ChangeTag, TextDiff};

const TEXT_EXTENSIONS: &[&str] = &[
    ".txt", ".md", ".rst", ".log", ".cfg", ".ini", ".toml", ".yaml", ".yml", ".json", ".xml",
    ".html", ".htm", ".css", ".js", ".py", ".rs", ".sh", ".sql", ".r", ".rb", ".pl", ".c", ".h",
    ".cpp", ".hpp", ".java",
];

/// Line-level diff comparator for text files.
pub struct TextComparator;

impl Comparator for TextComparator {
    fn name(&self) -> &str {
        "binoc.text"
    }

    fn handles_extensions(&self) -> &[&str] {
        TEXT_EXTENSIONS
    }

    fn can_handle(&self, pair: &ItemPair) -> bool {
        let item = pair.right.as_ref().or(pair.left.as_ref());
        if let Some(item) = item {
            if let Some(guess) = mime_guess::from_path(&item.logical_path).first() {
                return guess.type_() == mime_guess::mime::TEXT;
            }
        }
        false
    }

    fn reopen_data(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<ReopenedData> {
        let left = pair
            .left
            .as_ref()
            .map(|item| std::fs::read_to_string(&item.physical_path).map_err(BinocError::Io))
            .transpose()?;
        let right = pair
            .right
            .as_ref()
            .map(|item| std::fs::read_to_string(&item.physical_path).map_err(BinocError::Io))
            .transpose()?;
        Ok(ReopenedData::Text { left, right })
    }

    fn extract(
        &self,
        data: &ReopenedData,
        _node: &DiffNode,
        aspect: &str,
    ) -> Option<ExtractResult> {
        let ReopenedData::Text { left, right } = data else {
            return None;
        };
        match aspect {
            "diff" => {
                let l = left.as_deref().unwrap_or("");
                let r = right.as_deref().unwrap_or("");
                let diff = TextDiff::from_lines(l, r);
                let mut out = String::new();
                for change in diff.iter_all_changes() {
                    let sign = match change.tag() {
                        ChangeTag::Insert => "+",
                        ChangeTag::Delete => "-",
                        ChangeTag::Equal => " ",
                    };
                    out.push_str(&format!("{sign}{change}"));
                }
                Some(ExtractResult::Text(out))
            }
            "content_left" => left.as_ref().map(|s| ExtractResult::Text(s.clone())),
            "content_right" => right.as_ref().map(|s| ExtractResult::Text(s.clone())),
            "content" | "full" => {
                let mut out = String::new();
                if let Some(l) = left {
                    out.push_str("--- left\n");
                    out.push_str(l);
                    if !l.ends_with('\n') {
                        out.push('\n');
                    }
                }
                if let Some(r) = right {
                    out.push_str("+++ right\n");
                    out.push_str(r);
                    if !r.ends_with('\n') {
                        out.push('\n');
                    }
                }
                Some(ExtractResult::Text(out))
            }
            _ => None,
        }
    }

    fn compare(&self, pair: &ItemPair, _ctx: &CompareContext) -> BinocResult<CompareResult> {
        match (&pair.left, &pair.right) {
            (Some(left), Some(right)) => {
                let text_l =
                    std::fs::read_to_string(&left.physical_path).map_err(BinocError::Io)?;
                let text_r =
                    std::fs::read_to_string(&right.physical_path).map_err(BinocError::Io)?;

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
                let text = std::fs::read_to_string(&right.physical_path).map_err(BinocError::Io)?;
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
                let text = std::fs::read_to_string(&left.physical_path).map_err(BinocError::Io)?;
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
