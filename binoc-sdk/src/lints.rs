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

use crate::PairDescriptor;

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

/// Lint correspondence pair-rule descriptors. Pair evidence is an open
/// vocabulary, but each pair rule must declare the exact evidence strings it
/// can emit so the engine can fail closed when a rule drifts.
pub fn lint_pair_descriptors(descriptors: &[PairDescriptor]) -> LintReport {
    use std::collections::BTreeSet;

    let mut report = LintReport::default();
    for desc in descriptors {
        if desc.name.trim().is_empty() {
            report
                .errors
                .push("pair rule has an empty descriptor name".into());
        }
        if desc.emits.is_empty() {
            report.errors.push(format!(
                "pair rule '{}' declares no emitted evidence strings",
                desc.name
            ));
        }
        let mut seen = BTreeSet::new();
        for evidence in &desc.emits {
            if evidence.trim().is_empty() {
                report.errors.push(format!(
                    "pair rule '{}' declares an empty evidence string",
                    desc.name
                ));
            }
            if !seen.insert(evidence) {
                report.errors.push(format!(
                    "pair rule '{}' declares duplicate evidence '{}'",
                    desc.name, evidence
                ));
            }
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

/// Lint: rules must not erase or overwrite the tag set wholesale — tags are
/// facts owned by whichever rule pack set them. Targeted `tags.remove(...)` of
/// a fact whose truth the rule is changing is legitimate and not flagged.
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

    #[test]
    fn pair_descriptor_lint_flags_undeclared_empty_and_duplicate_evidence() {
        let report = lint_pair_descriptors(&[
            PairDescriptor {
                name: "missing-evidence".into(),
                emits: vec![],
                reads: vec![],
                sees_beneath_settled: false,
            },
            PairDescriptor {
                name: "bad-evidence".into(),
                emits: vec!["".into(), "hash".into(), "hash".into()],
                reads: vec![],
                sees_beneath_settled: false,
            },
            PairDescriptor {
                name: "".into(),
                emits: vec!["root".into()],
                reads: vec![],
                sees_beneath_settled: false,
            },
        ]);

        assert_eq!(report.errors.len(), 4, "errors: {:?}", report.errors);
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("declares no emitted evidence strings")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("declares an empty evidence string")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("declares duplicate evidence 'hash'")));
        assert!(report
            .errors
            .iter()
            .any(|error| error.contains("empty descriptor name")));
    }

    #[test]
    fn pair_descriptor_lint_accepts_declared_evidence() {
        let report = lint_pair_descriptors(&[PairDescriptor {
            name: "hash-pair".into(),
            emits: vec!["binoc.hash".into()],
            reads: vec![],
            sees_beneath_settled: false,
        }]);

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty());
    }
}
