//! Tree-wide fuzzy leaf correlation: pair `add`/`remove` leaves whose
//! content is similar but not byte-identical, so a file that was both
//! renamed and modified is reported as a single `move` with the content
//! diff underneath instead of an unrelated `remove` + `add` pair.
//!
//! Runs at the root of the tree, **after**
//! [`super::correlation_detector::CorrelationDetector`] (which has
//! already consumed exact-hash matches). What's left over for this pass
//! is the residual `add`/`remove` leaves the exact-hash correlator
//! couldn't pair.
//!
//! Pipeline:
//!
//! 1. Walk the tree, collect leaves with `action == "add"` or `"remove"`
//!    that have a usable `source_items` `ItemRef` on the appropriate
//!    side.
//! 2. Pre-filter each (remove, add) pair: extensions must match, byte
//!    sizes must be within a 10x ratio, neither side may look binary.
//! 3. Score the surviving pairs with a token-set Jaccard index over the
//!    file bytes (line-set Jaccard is too brittle for structured formats
//!    like CSV, where adding a column changes every line but most cell
//!    values are shared).
//! 4. Greedy assignment: sort by descending similarity, take the top
//!    matches above the threshold without reusing either side.
//! 5. For each match, plan a rewrite that removes the two leaves and
//!    inserts a single `move` node at the destination's parent with
//!    `pending_recompare` set to the reconstructed pair, so the
//!    controller will re-dispatch through the comparator pipeline and
//!    merge the resulting content diff into the move node.
//!
//! Config:
//!
//! ```json
//! {
//!   "enable": true,
//!   "threshold": 0.5,
//!   "rename_limit": 400,
//!   "size_ratio": 10.0
//! }
//! ```
//!
//! `enable = false` short-circuits the whole pass. `rename_limit` caps
//! the worst-case `O(adds * removes)` byte-reading work (matches Git's
//! `diff.renameLimit` default of 400).

use std::collections::{BTreeSet, HashSet};

use binoc_sdk::*;

use super::correlation::{apply_rewrite, parent_path_of, source_label_for_move, RewritePlan};

const DEFAULT_THRESHOLD: f64 = 0.5;
const DEFAULT_RENAME_LIMIT: usize = 400;
const DEFAULT_SIZE_RATIO: f64 = 10.0;
const LARGE_FILE_THRESHOLD: usize = 1_000_000;
const LARGE_FILE_SAMPLE_LINES: usize = 512;

pub struct FuzzyCorrelationDetector;

#[derive(Debug, Clone)]
struct Config {
    enable: bool,
    threshold: f64,
    rename_limit: usize,
    size_ratio: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable: true,
            threshold: DEFAULT_THRESHOLD,
            rename_limit: DEFAULT_RENAME_LIMIT,
            size_ratio: DEFAULT_SIZE_RATIO,
        }
    }
}

impl Config {
    fn from_value(v: &serde_json::Value) -> Self {
        let mut out = Self::default();
        if let Some(b) = v.get("enable").and_then(|x| x.as_bool()) {
            out.enable = b;
        }
        if let Some(t) = v.get("threshold").and_then(|x| x.as_f64()) {
            out.threshold = t.clamp(0.0, 1.0);
        }
        if let Some(n) = v.get("rename_limit").and_then(|x| x.as_u64()) {
            out.rename_limit = n as usize;
        }
        if let Some(r) = v.get("size_ratio").and_then(|x| x.as_f64()) {
            if r >= 1.0 {
                out.size_ratio = r;
            }
        }
        out
    }
}

impl Transformer for FuzzyCorrelationDetector {
    fn descriptor(&self) -> TransformerDescriptor {
        TransformerDescriptor::new("binoc.fuzzy_correlation_detector")
            .with_node_shape(NodeShapeFilter::Root)
    }

    fn transform(
        &self,
        node: DiffNode,
        data: &dyn DataAccess,
        config: &serde_json::Value,
    ) -> TransformResult {
        let cfg = Config::from_value(config);
        if !cfg.enable {
            return TransformResult::Unchanged;
        }

        let mut adds: Vec<FuzzyLeaf> = Vec::new();
        let mut removes: Vec<FuzzyLeaf> = Vec::new();
        collect_fuzzy_leaves(&node, &mut adds, &mut removes);

        if adds.is_empty() || removes.is_empty() {
            return TransformResult::Unchanged;
        }
        if adds.len().saturating_mul(removes.len()) > cfg.rename_limit {
            return TransformResult::Unchanged;
        }

        let pairs = score_pairs(&removes, &adds, &cfg, data);
        let chosen = greedy_assign(pairs, cfg.threshold);
        if chosen.is_empty() {
            return TransformResult::Unchanged;
        }

        let mut plan = RewritePlan::default();
        for (ri, ai) in chosen {
            let rm = &removes[ri];
            let add = &adds[ai];
            let move_node = build_move_node(rm, add);
            plan.schedule_remove(&rm.path);
            plan.schedule_remove(&add.path);
            plan.schedule_insert(parent_path_of(&add.path), move_node);
        }

        if plan.is_empty() {
            return TransformResult::Unchanged;
        }
        let rewritten = apply_rewrite(node, &plan);
        TransformResult::Replace(Box::new(rewritten))
    }
}

/// A residual `add` or `remove` leaf with everything fuzzy matching needs.
struct FuzzyLeaf {
    path: String,
    item_type: String,
    /// `ItemRef` for the leaf's content (`right` for adds, `left` for removes).
    item: ItemRef,
}

fn collect_fuzzy_leaves(node: &DiffNode, adds: &mut Vec<FuzzyLeaf>, removes: &mut Vec<FuzzyLeaf>) {
    if node.children.is_empty() {
        if let Some(entry) = as_fuzzy_leaf(node) {
            match node.action.as_str() {
                "add" => adds.push(entry),
                "remove" => removes.push(entry),
                _ => {}
            }
        }
        return;
    }
    for child in &node.children {
        collect_fuzzy_leaves(child, adds, removes);
    }
}

fn as_fuzzy_leaf(node: &DiffNode) -> Option<FuzzyLeaf> {
    let pair = node.source_items.as_ref()?;
    let item = match node.action.as_str() {
        "add" => pair.right.clone()?,
        "remove" => pair.left.clone()?,
        _ => return None,
    };
    if item.is_dir {
        return None;
    }
    Some(FuzzyLeaf {
        path: node.path.clone(),
        item_type: node.item_type.clone(),
        item,
    })
}

fn score_pairs(
    removes: &[FuzzyLeaf],
    adds: &[FuzzyLeaf],
    cfg: &Config,
    data: &dyn DataAccess,
) -> Vec<ScoredPair> {
    // Memoize byte reads per leaf so a single remove paired against many
    // adds (worst case up to `rename_limit`) reads each leaf's content at
    // most once. Outer `Option` is "tried to read yet"; inner is the read
    // result — failed reads are cached too so we don't retry.
    let mut left_cache: Vec<Option<Option<Vec<u8>>>> = vec![None; removes.len()];
    let mut right_cache: Vec<Option<Option<Vec<u8>>>> = vec![None; adds.len()];

    let mut out: Vec<ScoredPair> = Vec::new();
    for (ri, rm) in removes.iter().enumerate() {
        for (ai, add) in adds.iter().enumerate() {
            if !extensions_match(&rm.path, &add.path) {
                continue;
            }
            // Cheap pre-filter using cached sizes from `ItemRef` when
            // available; skips byte reads for obviously-mismatched pairs.
            if let (Some(l), Some(r)) = (rm.item.size, add.item.size) {
                if !sizes_within_ratio(Some(l), Some(r), cfg.size_ratio) {
                    continue;
                }
            }
            let Some(left_bytes) = read_cached(&mut left_cache, ri, &rm.item, data) else {
                continue;
            };
            let Some(right_bytes) = read_cached(&mut right_cache, ai, &add.item, data) else {
                continue;
            };
            // Definitive size check using actual lengths (covers the case
            // where one side's `ItemRef.size` wasn't pre-populated).
            if !sizes_within_ratio(
                Some(left_bytes.len() as u64),
                Some(right_bytes.len() as u64),
                cfg.size_ratio,
            ) {
                continue;
            }
            if is_binary(left_bytes) || is_binary(right_bytes) {
                continue;
            }
            let score = token_set_similarity(left_bytes, right_bytes);
            out.push(ScoredPair {
                remove_idx: ri,
                add_idx: ai,
                score,
            });
        }
    }
    out
}

/// Lazy-load bytes for the leaf at `idx` into `cache`, returning a slice
/// into the cached bytes (or `None` if the read failed). Each leaf is
/// read at most once across all calls.
fn read_cached<'a>(
    cache: &'a mut [Option<Option<Vec<u8>>>],
    idx: usize,
    item: &ItemRef,
    data: &dyn DataAccess,
) -> Option<&'a [u8]> {
    if cache[idx].is_none() {
        cache[idx] = Some(data.read_bytes(item).ok());
    }
    cache[idx].as_ref().and_then(|o| o.as_deref())
}

struct ScoredPair {
    remove_idx: usize,
    add_idx: usize,
    score: f64,
}

/// Greedy 1:1 assignment: sort pairs by descending score, then take each
/// in turn skipping any whose remove or add was already consumed. This
/// deliberately does not handle M:N matches — if a file is copied and
/// both copies are then edited, the result is one `move` + one new `add`,
/// not two `move`s. A 1:1 framing reads more naturally for the user
/// ("renamed and modified" + "new file") than reporting the same source
/// as the origin of two different destinations would.
fn greedy_assign(mut pairs: Vec<ScoredPair>, threshold: f64) -> Vec<(usize, usize)> {
    pairs.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut used_removes: BTreeSet<usize> = BTreeSet::new();
    let mut used_adds: BTreeSet<usize> = BTreeSet::new();
    let mut chosen = Vec::new();
    for p in pairs {
        if p.score < threshold {
            break;
        }
        if used_removes.contains(&p.remove_idx) || used_adds.contains(&p.add_idx) {
            continue;
        }
        used_removes.insert(p.remove_idx);
        used_adds.insert(p.add_idx);
        chosen.push((p.remove_idx, p.add_idx));
    }
    chosen
}

fn build_move_node(rm: &FuzzyLeaf, add: &FuzzyLeaf) -> DiffNode {
    let source_name = source_label_for_move(&rm.path, &add.path);
    let mut node = DiffNode::new("move", &add.item_type, &add.path)
        .with_summary(format!("Moved from {source_name} (modified)"))
        .with_source_path(&rm.path)
        .with_tag("binoc.move")
        .with_tag("binoc.move.modified");
    node.pending_recompare = Some(ItemPair::both(rm.item.clone(), add.item.clone()));
    node
}

/// Both files must share the same (lowercased) extension. If neither has
/// an extension (e.g. `Makefile` vs `Dockerfile`, `LICENSE` vs `NOTICE`),
/// the pair is allowed through to the similarity check — the Jaccard
/// threshold is responsible for rejecting unrelated content.
fn extensions_match(a: &str, b: &str) -> bool {
    fn ext(s: &str) -> Option<String> {
        s.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())
    }
    ext(a) == ext(b)
}

fn sizes_within_ratio(a: Option<u64>, b: Option<u64>, max_ratio: f64) -> bool {
    match (a, b) {
        (Some(0), Some(0)) => true,
        (Some(x), Some(y)) if x == 0 || y == 0 => false,
        (Some(x), Some(y)) => {
            let big = x.max(y) as f64;
            let small = x.min(y) as f64;
            big / small <= max_ratio
        }
        // If either side wouldn't resolve, fall through and let the byte
        // read decide (it'll most likely fail too, which is fine).
        _ => true,
    }
}

fn is_binary(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

/// Jaccard index on the set of distinct non-empty tokens. Tokens are
/// delimited by newlines, carriage returns, commas, tabs, and spaces —
/// chosen so that structured formats (CSV especially) get useful overlap
/// scores when whole lines change but cell values are shared.
fn token_set_similarity(left: &[u8], right: &[u8]) -> f64 {
    let left_sample = sample_bytes(left);
    let right_sample = sample_bytes(right);
    let set_a: HashSet<&[u8]> = tokenize(left_sample).filter(|t| !t.is_empty()).collect();
    let set_b: HashSet<&[u8]> = tokenize(right_sample).filter(|t| !t.is_empty()).collect();

    let inter = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 1.0;
    }
    inter as f64 / union as f64
}

fn tokenize(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    data.split(|b| matches!(*b, b'\n' | b'\r' | b',' | b'\t' | b' '))
}

/// For large files, sample roughly the first `LARGE_FILE_SAMPLE_LINES`
/// lines. Bounds memory + CPU while still catching header/preamble
/// changes common in CSV column additions.
fn sample_bytes(data: &[u8]) -> &[u8] {
    if data.len() <= LARGE_FILE_THRESHOLD {
        return data;
    }
    let mut lines = 0usize;
    let mut end = data.len();
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            lines += 1;
            if lines >= LARGE_FILE_SAMPLE_LINES {
                end = i + 1;
                break;
            }
        }
    }
    &data[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = Config::from_value(&serde_json::Value::Null);
        assert!(cfg.enable);
        assert_eq!(cfg.threshold, DEFAULT_THRESHOLD);
        assert_eq!(cfg.rename_limit, DEFAULT_RENAME_LIMIT);
        assert_eq!(cfg.size_ratio, DEFAULT_SIZE_RATIO);
    }

    #[test]
    fn config_disable() {
        let cfg = Config::from_value(&serde_json::json!({ "enable": false }));
        assert!(!cfg.enable);
    }

    #[test]
    fn config_threshold_clamps() {
        let cfg = Config::from_value(&serde_json::json!({ "threshold": 2.0 }));
        assert_eq!(cfg.threshold, 1.0);
        let cfg = Config::from_value(&serde_json::json!({ "threshold": -1.0 }));
        assert_eq!(cfg.threshold, 0.0);
    }

    #[test]
    fn extensions_match_same() {
        assert!(extensions_match("data.csv", "data_v2.csv"));
    }

    #[test]
    fn extensions_match_different() {
        assert!(!extensions_match("data.csv", "data.txt"));
    }

    #[test]
    fn extensions_match_none_on_both() {
        assert!(extensions_match("Makefile", "OtherFile"));
    }

    #[test]
    fn sizes_within_ratio_close() {
        assert!(sizes_within_ratio(Some(100), Some(150), 10.0));
    }

    #[test]
    fn sizes_within_ratio_too_far() {
        assert!(!sizes_within_ratio(Some(100), Some(2000), 10.0));
    }

    #[test]
    fn sizes_within_ratio_zero_zero_ok() {
        assert!(sizes_within_ratio(Some(0), Some(0), 10.0));
    }

    #[test]
    fn sizes_within_ratio_one_zero_rejects() {
        assert!(!sizes_within_ratio(Some(0), Some(100), 10.0));
    }

    #[test]
    fn is_binary_detects_null() {
        assert!(is_binary(b"hello\x00world"));
    }

    #[test]
    fn is_binary_text_ok() {
        assert!(!is_binary(b"hello, world\n"));
    }

    #[test]
    fn token_similarity_identical() {
        let data = b"hello\nworld\n";
        assert!((token_set_similarity(data, data) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn token_similarity_disjoint() {
        let a = b"aaa\nbbb\n";
        let b = b"ccc\nddd\n";
        assert!(token_set_similarity(a, b) < f64::EPSILON);
    }

    #[test]
    fn token_similarity_csv_column_addition() {
        let a = b"name,age,city\nAlice,30,Portland\nBob,25,Seattle\n";
        let b = b"name,age,city,email\nAlice,30,Portland,a@t.com\nBob,25,Seattle,b@t.com\n";
        let sim = token_set_similarity(a, b);
        assert!(
            sim > 0.5,
            "CSV column addition should score >0.5, got {sim}"
        );
    }

    #[test]
    fn token_similarity_partial_text_overlap() {
        let a = b"line one\nline two\nline three\n";
        let b = b"line one\nline two\nline four\n";
        let sim = token_set_similarity(a, b);
        assert!(sim > 0.5 && sim < 1.0);
    }

    #[test]
    fn greedy_assign_picks_highest_first() {
        let pairs = vec![
            ScoredPair {
                remove_idx: 0,
                add_idx: 0,
                score: 0.6,
            },
            ScoredPair {
                remove_idx: 0,
                add_idx: 1,
                score: 0.9,
            },
            ScoredPair {
                remove_idx: 1,
                add_idx: 0,
                score: 0.7,
            },
        ];
        let chosen = greedy_assign(pairs, 0.5);
        assert_eq!(chosen, vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn greedy_assign_respects_threshold() {
        let pairs = vec![ScoredPair {
            remove_idx: 0,
            add_idx: 0,
            score: 0.4,
        }];
        assert!(greedy_assign(pairs, 0.5).is_empty());
    }

    // ── score_pairs byte-read memoization ──────────────────────────────

    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    /// Minimal `DataAccess` stub: serves bytes for a fixed in-memory
    /// table and counts every `read_bytes` call per logical path. Other
    /// methods panic so test failures point at unexpected behavior.
    struct CountingDataAccess {
        bytes: HashMap<String, Vec<u8>>,
        reads: Mutex<HashMap<String, usize>>,
    }

    impl CountingDataAccess {
        fn new(entries: Vec<(String, Vec<u8>)>) -> Self {
            Self {
                bytes: entries.into_iter().collect(),
                reads: Mutex::new(HashMap::new()),
            }
        }
        fn read_count(&self, key: &str) -> usize {
            *self.reads.lock().unwrap().get(key).unwrap_or(&0)
        }
    }

    impl DataAccess for CountingDataAccess {
        fn read_bytes(&self, item: &ItemRef) -> BinocResult<Vec<u8>> {
            *self
                .reads
                .lock()
                .unwrap()
                .entry(item.logical_path.clone())
                .or_insert(0) += 1;
            self.bytes
                .get(&item.logical_path)
                .cloned()
                .ok_or_else(|| BinocError::Other(format!("no bytes for {}", item.logical_path)))
        }
        fn open_read(&self, _: &ItemRef) -> BinocResult<Box<dyn std::io::Read + Send>> {
            unimplemented!()
        }
        fn local_path(&self, _: &ItemRef) -> BinocResult<PathBuf> {
            unimplemented!()
        }
        fn provide(&self, _: &str, _: &[u8]) -> BinocResult<ItemRef> {
            unimplemented!()
        }
        fn workspace(&self) -> BinocResult<PathBuf> {
            unimplemented!()
        }
        fn register_local(&self, _: &Path, _: &str) -> BinocResult<ItemRef> {
            unimplemented!()
        }
        fn publish_artifact(
            &self,
            _: &ArtifactFormat,
            _: ArtifactSubject,
            _: &str,
            _: &[u8],
        ) -> BinocResult<ArtifactDescriptor> {
            unimplemented!()
        }
        fn get_artifact(&self, _: &ArtifactDescriptor) -> BinocResult<Option<Vec<u8>>> {
            unimplemented!()
        }
        fn data_root(&self) -> BinocResult<PathBuf> {
            unimplemented!()
        }
    }

    fn leaf(path: &str, item_type: &str, size: Option<u64>) -> FuzzyLeaf {
        FuzzyLeaf {
            path: path.to_string(),
            item_type: item_type.to_string(),
            item: ItemRef {
                logical_path: path.to_string(),
                is_dir: false,
                content_hash: None,
                size,
                media_type: None,
                handle: String::new(),
            },
        }
    }

    #[test]
    fn score_pairs_reads_each_leaf_at_most_once() {
        // One remove paired against many adds. Without memoization the
        // remove's bytes would be re-read for every add candidate (N
        // reads); with memoization it should be exactly one.
        let removes = vec![leaf("old.csv", "tabular", None)];
        let adds: Vec<FuzzyLeaf> = (0..5)
            .map(|i| leaf(&format!("new_{i}.csv"), "tabular", None))
            .collect();

        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        entries.push(("old.csv".into(), b"name,age\nalice,30\nbob,25\n".to_vec()));
        for i in 0..5 {
            entries.push((
                format!("new_{i}.csv"),
                format!("name,age\nalice,30\nbob,2{i}\n").into_bytes(),
            ));
        }
        let data = CountingDataAccess::new(entries);
        let cfg = Config::default();
        let _ = score_pairs(&removes, &adds, &cfg, &data);

        assert_eq!(
            data.read_count("old.csv"),
            1,
            "remove side should be read at most once across all candidate pairs"
        );
        for i in 0..5 {
            assert_eq!(
                data.read_count(&format!("new_{i}.csv")),
                1,
                "each add candidate should be read at most once"
            );
        }
    }

    #[test]
    fn score_pairs_cached_size_skips_byte_read() {
        // When `ItemRef.size` is pre-populated and the ratio fails, we
        // shouldn't read either side's bytes at all.
        let removes = vec![leaf("tiny.csv", "tabular", Some(10))];
        let adds = vec![leaf("huge.csv", "tabular", Some(10_000_000))];
        let data = CountingDataAccess::new(vec![
            ("tiny.csv".into(), b"a,b,c\n".to_vec()),
            ("huge.csv".into(), b"a,b,c,d,e\n".to_vec()),
        ]);
        let cfg = Config::default();
        let _ = score_pairs(&removes, &adds, &cfg, &data);

        assert_eq!(data.read_count("tiny.csv"), 0);
        assert_eq!(data.read_count("huge.csv"), 0);
    }
}
