//! JIT, format-owned partition identities for N<->M correspondence (CFM-72).
//!
//! Some changes turn one artifact into several of the same shape (a table split
//! by year) or several into one (merged). Representing that needs a way to ask
//! whether one artifact's atomic sub-units are exactly the disjoint union of
//! several others'. This module supplies the seam:
//!
//! - [`IdentityToken`] — an opaque, globally-comparable identity for one atomic
//!   sub-unit (e.g. a table row).
//! - [`IdentityExtractor`] — a format-keyed capability that yields an artifact's
//!   ordered token sequence. The engine dispatches it like writers/compaction;
//!   the *format* owns what a sub-unit is and how its identity is derived.
//! - [`disjoint_cover`] — the generic, opaque-token coverage query that answers
//!   "is `whole` the clean disjoint union of a subset of `pool`?".
//!
//! The query is deliberately conservative (see the partition-identities ADR): it
//! reports [`Coverage::Clean`] only when the relationship is complete (residual
//! 0), disjoint, unambiguous, and not a whole-artifact 1:1; any messiness is a
//! [`Coverage::NearMiss`] that a consumer declines on, leaving honest add/remove.

use std::collections::HashMap;

use crate::{tabular_v1, ArtifactFormat, BinocError, BinocResult, TabularData};

use serde::{Deserialize, Serialize};

/// An opaque, globally-comparable identity for one atomic sub-unit of an
/// artifact (e.g. a table row).
///
/// The engine only ever compares tokens for equality / membership /
/// disjointness — it never interprets their contents. The producing artifact
/// *format* owns the meaning (content hash, stable key, …) via its
/// [`IdentityExtractor`]. Tokens must be globally comparable: the same sub-unit
/// in two different artifacts must yield the same token (content- or
/// key-derived, never a positional index).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IdentityToken(pub String);

impl IdentityToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Metadata for a registered [`IdentityExtractor`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct IdentityExtractorDescriptor {
    pub name: String,
    /// The artifact format whose atomic sub-units this extractor identifies.
    pub format: ArtifactFormat,
}

/// Derives an ordered sequence of opaque [`IdentityToken`]s for an artifact —
/// the atomic sub-units (e.g. table rows) used by partition (split/merge)
/// detection.
///
/// Keyed by [`ArtifactFormat`] and dispatched like writers/compaction/annotators;
/// a format with no registered extractor is simply not partition-capable. The
/// extractor rides the *format*, so every producer of that format gains
/// partition capability for free — the parsers do nothing.
pub trait IdentityExtractor: Send + Sync {
    fn descriptor(&self) -> IdentityExtractorDescriptor;
    /// Yield the ordered identity tokens for one artifact's serialized bytes.
    fn extract(&self, artifact_bytes: &[u8]) -> BinocResult<Vec<IdentityToken>>;
}

/// The SDK's `tabular_v1` identity extractor: one token per row, derived from the
/// row's cell values (order-stable canonical JSON of the `Vec<Value>`). All six
/// `tabular_v1` producers (CSV, SQLite, Excel, Parquet, Avro, DBF) gain partition
/// capability through this single extractor.
pub struct TabularIdentityExtractor;

impl IdentityExtractor for TabularIdentityExtractor {
    fn descriptor(&self) -> IdentityExtractorDescriptor {
        IdentityExtractorDescriptor {
            name: "binoc.identity.tabular".into(),
            format: tabular_v1(),
        }
    }

    fn extract(&self, artifact_bytes: &[u8]) -> BinocResult<Vec<IdentityToken>> {
        let table: TabularData = serde_json::from_slice(artifact_bytes).map_err(|err| {
            BinocError::Other(format!("decode tabular artifact for identity: {err}"))
        })?;
        // The token is the row's cell values only — not the header — so the same
        // row recognizes across a reformat that reorders/renames columns is left
        // to the fuzzy tier; here equality is exact cell content.
        Ok(table
            .rows
            .iter()
            .map(|row| {
                let canonical = serde_json::to_string(row).unwrap_or_default();
                IdentityToken::new(canonical)
            })
            .collect())
    }
}

/// One participant in a coverage query: an opaque node handle plus its ordered
/// identity tokens.
pub struct Candidate<T> {
    pub node: T,
    pub tokens: Vec<IdentityToken>,
}

/// A clean, complete, disjoint, unambiguous partition: `whole`'s token multiset
/// is exactly the disjoint union of `parts`, each a strict subset.
pub struct PartitionMatch<T> {
    pub whole: T,
    pub parts: Vec<T>,
    /// Number of atoms (tokens) the partition covers — `whole`'s total.
    pub covered: usize,
}

/// Outcome of [`disjoint_cover`].
pub enum Coverage<T> {
    /// A clean partition: complete (residual 0), disjoint, unambiguous, ≥2 parts,
    /// and no single part equals the whole.
    Clean(PartitionMatch<T>),
    /// `whole` shares atoms with `pool` members but the relationship is not clean
    /// — a residual, a shared (ambiguous) token, or a foreign atom. A
    /// conservative consumer declines and reports `binoc.possible_split`.
    NearMiss,
    /// `whole` shares no atoms with any `pool` member — unrelated.
    None,
}

/// Is `whole`'s token multiset the **clean disjoint union** of a subset of
/// `pool`? Generic over opaque tokens and location-agnostic — `pool` may live
/// anywhere in either tree, because the tokens carry the correspondence, not the
/// structure. Call it with `whole` = an input and `pool` = the output residue to
/// detect a split; swap the sides to detect a merge.
///
/// Conservative by construction: any pool member that shares at least one token
/// with `whole` is treated as a participant, and the cover is [`Coverage::Clean`]
/// only when **every** participant is fully inside `whole` (no foreign atoms),
/// participants are pairwise disjoint (no token owned by two — the ambiguity
/// flag), their union reconstructs `whole` exactly (residual 0), there are at
/// least two of them, and none equals the whole. Anything else is a
/// [`Coverage::NearMiss`].
pub fn disjoint_cover<T: Clone>(whole: &Candidate<T>, pool: &[Candidate<T>]) -> Coverage<T> {
    if whole.tokens.is_empty() {
        return Coverage::None;
    }
    let mut whole_counts: HashMap<&IdentityToken, usize> = HashMap::new();
    for token in &whole.tokens {
        *whole_counts.entry(token).or_default() += 1;
    }

    // A pool member participates if it shares any atom with the whole.
    let participants: Vec<usize> = pool
        .iter()
        .enumerate()
        .filter(|(_, cand)| {
            !cand.tokens.is_empty() && cand.tokens.iter().any(|t| whole_counts.contains_key(t))
        })
        .map(|(index, _)| index)
        .collect();
    if participants.is_empty() {
        return Coverage::None;
    }

    // Walk every participant atom once, checking three cleanliness properties:
    // foreign atoms (a participant token not in the whole), ambiguity (a token
    // owned by two participants), and coverage (participant union == whole).
    let mut owner: HashMap<&IdentityToken, usize> = HashMap::new();
    let mut covered: HashMap<&IdentityToken, usize> = HashMap::new();
    let mut clean = true;
    for &index in &participants {
        for token in &pool[index].tokens {
            if !whole_counts.contains_key(token) {
                clean = false; // foreign atom
            }
            match owner.get(token) {
                Some(&existing) if existing != index => clean = false, // ambiguous
                _ => {
                    owner.insert(token, index);
                }
            }
            *covered.entry(token).or_default() += 1;
        }
    }
    if clean {
        for (token, &want) in &whole_counts {
            if covered.get(token).copied().unwrap_or(0) != want {
                clean = false; // residual / over-cover
                break;
            }
        }
    }
    if participants.len() < 2 {
        // A single participant is a 1:1 relationship — a clean whole-artifact
        // move/copy, or a modify with shared-and-changed rows. Either way the
        // exact/fuzzy rules own it; it is neither a split nor a near miss worth a
        // diagnostic. Only ≥2 other-side tables that *together* almost reconstruct
        // the whole are a genuine partition (or near miss).
        return Coverage::None;
    }
    if !clean {
        return Coverage::NearMiss;
    }

    Coverage::Clean(PartitionMatch {
        whole: whole.node.clone(),
        parts: participants
            .iter()
            .map(|&index| pool[index].node.clone())
            .collect(),
        covered: whole.tokens.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(node: &str, rows: &[&str]) -> Candidate<String> {
        Candidate {
            node: node.to_string(),
            tokens: rows.iter().map(|r| IdentityToken::new(*r)).collect(),
        }
    }

    #[test]
    fn clean_split_is_detected() {
        let whole = cand("all", &["a", "b", "c", "d"]);
        let pool = vec![cand("x", &["a", "b"]), cand("y", &["c", "d"])];
        match disjoint_cover(&whole, &pool) {
            Coverage::Clean(m) => {
                assert_eq!(m.covered, 4);
                assert_eq!(m.parts.len(), 2);
            }
            _ => panic!("expected clean split"),
        }
    }

    #[test]
    fn residual_is_near_miss() {
        // `d` is unaccounted for by the parts.
        let whole = cand("all", &["a", "b", "c", "d"]);
        let pool = vec![cand("x", &["a", "b"]), cand("y", &["c"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::NearMiss));
    }

    #[test]
    fn shared_token_is_ambiguous_near_miss() {
        // `b` appears in both parts.
        let whole = cand("all", &["a", "b", "c"]);
        let pool = vec![cand("x", &["a", "b"]), cand("y", &["b", "c"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::NearMiss));
    }

    #[test]
    fn foreign_atom_is_near_miss() {
        // `y` carries `z`, which is not in the whole — split-plus-edit, deferred.
        let whole = cand("all", &["a", "b", "c"]);
        let pool = vec![cand("x", &["a"]), cand("y", &["b", "c", "z"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::NearMiss));
    }

    #[test]
    fn single_participant_partial_is_not_a_split() {
        // A 1:1 reformat/modify: one other-side table shares some rows and changes
        // others. That is not a partition near miss — only the fuzzy rules' job.
        let whole = cand("data.csv", &["a", "b", "c"]);
        let pool = vec![cand("data.tsv", &["a", "c", "d"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::None));
    }

    #[test]
    fn unrelated_pool_is_none() {
        let whole = cand("all", &["a", "b"]);
        let pool = vec![cand("x", &["m", "n"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::None));
    }

    #[test]
    fn single_whole_cover_is_not_a_split() {
        // One part reconstructs the whole — that's a 1:1 move, not a split, and
        // not a near miss (no diagnostic): the exact rules own it.
        let whole = cand("all", &["a", "b"]);
        let pool = vec![cand("x", &["a", "b"])];
        assert!(matches!(disjoint_cover(&whole, &pool), Coverage::None));
    }
}
