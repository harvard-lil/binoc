//! Cheap, mechanical lints over plugin descriptors and source trees.
//!
//! Behind the `test-support` feature: lints are a development-time check,
//! never a runtime mechanism (the same posture as write-set enforcement —
//! see the write-sets ADR's never-for-scheduling constraint).
//!
//! Binoc checks invariants at three tiers; this module is the middle one:
//!
//! 1. **Harness invariants** — properties of every produced changeset,
//!    checked on every test vector by the shared harness
//!    (`binoc_stdlib::test_vectors`): changeset structural invariants and
//!    per-call write-set enforcement ([`crate::test_support`]). Hard
//!    failures.
//! 2. **Mechanical lints (this module)** — static checks over descriptor
//!    data and source text. Each lint returns a [`LintReport`]; errors
//!    fail the test, warnings print and pass. The convention is one
//!    `tests/lints.rs` per crate that calls these helpers — copy the
//!    stdlib's as a starting point, and run `just lint` to see warnings
//!    (libtest hides stderr from passing tests otherwise).
//! 3. **Agent lints** — invariants that need judgment rather than
//!    mechanism (behavioral completeness, performance, security,
//!    layering), documented as a review checklist in
//!    `.agents/skills/lint-plugin/SKILL.md`.
//!
//! Source-scan lints support an escape hatch: a line is exempt when it or
//! the line directly above contains `binoc-lint: allow(<rule>)`, ideally
//! with a short justification.

use std::path::Path;

use crate::traits::TransformerDescriptor;

/// Outcome of one or more lints. `errors` should fail the build;
/// `warnings` are advisory.
#[derive(Debug, Default)]
pub struct LintReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl LintReport {
    pub fn merge(&mut self, other: LintReport) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }

    /// Print warnings to stderr (visible under `--nocapture`, e.g. via
    /// `just lint`) and panic if there are errors. Call this at the end
    /// of a lint test.
    pub fn assert_clean(&self) {
        for warning in &self.warnings {
            eprintln!("warning: binoc-lint: {warning}");
        }
        assert!(
            self.errors.is_empty(),
            "binoc-lint errors:\n  {}",
            self.errors.join("\n  ")
        );
    }
}

// ── Descriptor lints ──────────────────────────────────────────────────

/// Lint: flag tags that exactly one transformer declares in `emits_tags`
/// and exactly one *other* transformer lists in `match_tags`.
///
/// Such a tag is a single-producer/single-consumer dispatch channel — a
/// function call drawn slowly. Consider inlining the consumer's judgment
/// into the producer, or documenting why it stays separate (e.g. the tag
/// is also consumed outside transformer dispatch, such as by renderer
/// group configs — pass those tags in `allowlist`).
///
/// Returns one warning string per flagged tag. Callers decide whether to
/// fail or just print; undeclared (`None`) emits_tags are skipped.
pub fn single_producer_single_consumer_tags(
    descriptors: &[TransformerDescriptor],
    allowlist: &[&str],
) -> Vec<String> {
    use std::collections::BTreeMap;

    let mut producers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut consumers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for desc in descriptors {
        if let Some(tags) = &desc.emits_tags {
            for tag in tags {
                producers.entry(tag).or_default().push(&desc.name);
            }
        }
        for tag in &desc.match_tags {
            consumers.entry(tag).or_default().push(&desc.name);
        }
    }

    let mut warnings = Vec::new();
    for (tag, tag_producers) in &producers {
        if allowlist.contains(tag) {
            continue;
        }
        let Some(tag_consumers) = consumers.get(tag) else {
            continue;
        };
        if tag_producers.len() == 1
            && tag_consumers.len() == 1
            && tag_producers[0] != tag_consumers[0]
        {
            warnings.push(format!(
                "tag '{tag}' is produced only by '{}' and consumed only by '{}' — \
                 single-producer/single-consumer tag; consider inlining the judgment \
                 or documenting why it stays separate",
                tag_producers[0], tag_consumers[0]
            ));
        }
    }
    warnings
}

/// Run the standard descriptor lints over a registry's transformer
/// descriptors: single-producer/single-consumer tags (warning, modulo
/// `spsc_allowlist`) and undeclared write-sets (warning — crates that
/// require full declarations, like binoc-stdlib, should assert that
/// separately as a hard test).
pub fn lint_transformer_descriptors(
    descriptors: &[TransformerDescriptor],
    spsc_allowlist: &[&str],
) -> LintReport {
    let mut report = LintReport::default();
    report.warnings.extend(single_producer_single_consumer_tags(
        descriptors,
        spsc_allowlist,
    ));
    for desc in descriptors {
        if desc.emits_tags.is_none()
            || desc.emits_actions.is_none()
            || desc.emits_item_types.is_none()
            || desc.publishes_artifacts.is_none()
        {
            report.warnings.push(format!(
                "transformer '{}' does not declare all write-sets \
                 (emits_tags/emits_actions/emits_item_types/publishes_artifacts) — \
                 legacy-undeclared plugins are exempt from harness enforcement",
                desc.name
            ));
        }
    }
    report
}

// ── Source-scan lints ─────────────────────────────────────────────────

/// Scan all `.rs` files under `root` for lines containing any of
/// `patterns` (plain substring match). Each hit becomes an **error**
/// citing file, line, and `why`, unless the line or the line directly
/// above carries `binoc-lint: allow(<rule>)`.
pub fn forbid_source_patterns(root: &Path, rule: &str, patterns: &[&str], why: &str) -> LintReport {
    let mut report = LintReport::default();
    let allow_marker = format!("binoc-lint: allow({rule})");
    for file in rust_files(root) {
        let Ok(contents) = std::fs::read_to_string(&file) else {
            report
                .errors
                .push(format!("unreadable source file: {}", file.display()));
            continue;
        };
        let lines: Vec<&str> = contents.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            if !patterns.iter().any(|p| line.contains(p)) {
                continue;
            }
            let allowed = line.contains(&allow_marker)
                || idx
                    .checked_sub(1)
                    .is_some_and(|prev| lines[prev].contains(&allow_marker));
            if !allowed {
                report.errors.push(format!(
                    "{}:{}: {why} (rule '{rule}'; suppress with `// {allow_marker}` if intentional)",
                    file.display(),
                    idx + 1,
                ));
            }
        }
    }
    report
}

/// Lint: transformers must not erase or overwrite the tag set wholesale —
/// tags are facts owned by whichever plugin set them. Targeted
/// `tags.remove(...)` of a fact whose truth the transformer is changing
/// is legitimate and not flagged. This is the `ColumnReorderDetector`
/// `tags.clear()` bug class.
pub fn forbid_tag_wipes(src_root: &Path) -> LintReport {
    forbid_source_patterns(
        src_root,
        "tag-wipe",
        &[".tags.clear()", ".tags = "],
        "wholesale tag clear/overwrite erases facts owned by other plugins",
    )
}

/// Lint: dispatch and scheduling code must never read write-set
/// declarations — they exist for verification, lint, and capability
/// negotiation only (see the write-sets ADR). Point this at
/// `binoc-core/src`.
pub fn forbid_write_set_reads(dispatch_src_root: &Path) -> LintReport {
    forbid_source_patterns(
        dispatch_src_root,
        "write-set-dispatch",
        &[
            "emits_tags",
            "emits_actions",
            "emits_item_types",
            "publishes_artifacts",
        ],
        "write-set declarations must never drive scheduling or dispatch",
    )
}

pub(crate) fn rust_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint_source(source: &str) -> LintReport {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plugin.rs"), source).unwrap();
        forbid_tag_wipes(dir.path())
    }

    #[test]
    fn tag_wipe_is_an_error_naming_file_and_line() {
        let report = lint_source("fn f(node: &mut DiffNode) {\n    node.tags.clear();\n}\n");
        assert_eq!(report.errors.len(), 1, "errors: {:?}", report.errors);
        assert!(report.errors[0].contains("plugin.rs:2"));
        assert!(report.errors[0].contains("tag-wipe"));
    }

    #[test]
    fn allow_comment_suppresses_on_same_or_preceding_line() {
        let same_line =
            lint_source("    node.tags.clear(); // binoc-lint: allow(tag-wipe) test fixture\n");
        assert!(same_line.errors.is_empty(), "{:?}", same_line.errors);

        let preceding = lint_source(
            "    // binoc-lint: allow(tag-wipe) test fixture\n    node.tags.clear();\n",
        );
        assert!(preceding.errors.is_empty(), "{:?}", preceding.errors);
    }

    #[test]
    fn clean_source_passes() {
        let report = lint_source("fn f(node: &mut DiffNode) {\n    node.tags.remove(\"x\");\n}\n");
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty());
    }
}
