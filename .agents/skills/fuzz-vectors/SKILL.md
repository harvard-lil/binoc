---
name: fuzz-vectors
description: AI "fuzzing" for binoc. A naive idea-generation subagent invents dataset-change scenarios from a bare description of what binoc is for — deliberately blind to binoc's actual formats, rules, and test vectors — then the orchestrator builds each scenario, runs binoc, and judges whether the output is good or reveals a bug or missing feature. Use to find things that ought to work but nobody designed for.
---

# Fuzz binoc with naive vectors

The goal is to discover changes a real user would expect binoc to handle well,
but that binoc handles badly, generically, or not at all — **without** letting
knowledge of binoc's design steer the search. The bugs and gaps worth finding
are the ones nobody on the team thought to write a test vector for.

This runs in two tiers:

1. **Generate (naive).** A subagent invents scenarios from a bare description of
   binoc's *purpose only*. It must not know which formats binoc supports, which
   rules exist, or what the test vectors cover.
2. **Test & report (informed).** The orchestrator materializes each scenario,
   runs binoc, and judges the output against the naive expectation the generator
   recorded — then writes up findings ranked by severity.

## Why tier 1 must be a subagent

By the time you read this skill you have almost certainly read binoc's docs,
source, or test-vector gallery. That makes *you* a biased idea generator: you
will unconsciously propose scenarios binoc already handles and skip the ones it
doesn't. The naivety has to come from context isolation — a fresh subagent that
has seen nothing but the briefing below. **Do not generate scenarios yourself.**
Delegate tier 1, every time.

## Tier 1 — generate scenarios

Spawn **3–5 idea-generation subagents in parallel** (one message, multiple
`Agent` calls), each seeded with a different *dataset world* so the ideas span
domains and file types instead of all converging on CSVs. Example seeds — pick
varied ones, invent your own:

- a city / government open-data portal
- a genomics or ecology research group
- a financial or regulatory filing archive
- a linguistics / text-corpus project
- a logistics / IoT / sensor operation
- a media or document archive (reports, images, scans)

Give each subagent **only** this briefing and nothing else about binoc. If possible, call them in such a way that AGENTS.md and this skill are not injected (such as by setting up a temp dir for them and calling out to a command line `codex`):

> **Briefing — what binoc is**
>
> Binoc is a command-line tool that produces a changelog for a dataset that
> doesn't ship one. You give it two snapshots of a dataset and it tells you, in
> meaningful terms, what changed between them. A snapshot is a folder of files
> representing the dataset's state at one point in time (a published versioned release, or a repeatedly run collection script, or
> a re-download with possible unannounced changes).
>
> Binoc is primarily intended for comparing versions of the same
> data (data added or removed, formatting changed) rather than
> vintages (last year's data vs this year's data). For now focus
> on modification scenarios rather than vintage scenarios.
>
> Binoc aims to be *format-aware*: rather than reporting "the bytes differ," it
> should describe a change the way someone who works with that kind of data
> would describe it — a record was added, a value was corrected, a file was
> renamed, a section was reorganized, a column was renamed.
> This typically maps to an inference about *how* the change
> was performed. It renders the
> result as a human-readable changelog, or as structured JSON.
>
> Its users track changes to published datasets: archivists, data scientists,
> journalists, civic-tech and public-interest researchers, and engineers feeding
> the changes into a pipeline, for example. You invoke it as
> `binoc diff <snapshot-a-folder> <snapshot-b-folder>`.

Instruct each generator to:

- **Imagine its assigned dataset world** and the realistic ways such a dataset
  changes between two releases. Cover happy paths *and* awkward-but-reasonable
  cases: re-encoding (UTF-8 -> UTF-16, BOM added), line-ending flips, a file
  split into many or many merged into one, a column's units or meaning changing,
  precision/rounding shifts, near-duplicate records, reordered keys, an entire
  release re-exported with cosmetic-only differences, nested or unusual
  containers, mixed-format bundles, empty/whitespace files, very large vs. tiny.
- **Not** worry about whether binoc can do it. Naivety is the point — propose
  what a reasonable user would simply expect to work.
- **NOT read any files in the repository, run any commands, or look up binoc.**
  Generate purely from the briefing. (Say this explicitly; it is the guardrail.)
- Make each scenario small and self-contained so it can be built in seconds.
- Think within and beyond listed examples: the goal is to map *all* changes a
  real user might encounter.
- Return a JSON array (and nothing else) of scenario objects:

```json
[
  {
    "id": "kebab-case-slug",
    "title": "one line",
    "domain": "the seed world",
    "change": "what differs between the two snapshots, in user terms",
    "snapshot_a": [{"path": "data.json", "text": "..."}],
    "snapshot_b": [{"path": "data.json", "text": "..."}],
    "expected": "what a user would expect binoc to SAY about this change",
    "why": "why a user expects that — the user-facing stake"
  }
]
```

For files that can't be expressed as text (zips, tarballs, sqlite, images,
spreadsheets, gzip), the generator may instead give
`{"path": "...", "build": "<shell command that creates the file in the cwd>"}`
for either side. Keep build commands to standard tools (`zip`, `tar`, `gzip`,
`sqlite3`, `printf`, ImageMagick if present).

Collect all arrays into one deduplicated list. Aim for ~20–40 scenarios total;
the user may ask for more or fewer.

## Tier 2 — test and judge

Work in a scratch directory outside the repo. Never write scenario files into
the working tree and never commit them.

Preprocess the scenarios to deduplicate and combine: try each identified kind of change separately, *and* create stress tests that represent realistic batches of changes that could appear within a single snapshot. Binoc should be robust to layered changes in large heterogeneous snapshots.

For each scenario:

```bash
ROOT=$(mktemp -d)              # one root for the whole run
S="$ROOT/<id>"; mkdir -p "$S/a" "$S/b"
# write each snapshot_a file under $S/a, each snapshot_b file under $S/b
# (run any `build` commands with cwd in the right side dir)
cd /Users/jcushman/Documents/binoc
just binoc diff "$S/a" "$S/b"            # default, no config — what a naive user runs
```

Notes on running:

- `just binoc …` runs the local source with the bundled plugins. Stale
  `Failed to load binoc plugin …` tracebacks on stderr are environmental cache
  noise — ignore them unless the diff itself fails. Run `just build` once up
  front if they bother you.
- The **default, no-config** invocation is the headline verdict, because that is
  what a naive user runs. Capture the verbatim output.
- To separate true *core/stdlib* gaps from "a bundled plugin saved it," re-run a
  borderline case with stdlib only:
  `uv run --with ./binoc-python binoc diff "$S/a" "$S/b"`.
- Also try `--format json` when the Markdown summary is too terse to judge
  correctness (e.g. it says "2 edits" without saying which).

Judge each result against the scenario's recorded `expected`, and classify:

| Verdict | Meaning |
|---|---|
| ✅ works | Output matches the naive expectation: correct, clear, useful. |
| 🟡 underwhelming | A real change was detected but described more vaguely than a user would hope (e.g. "2 edits" instead of "1 row added, 1 cell corrected"). A UX / feature gap, not wrong. |
| 🔵 defensible difference | Output differs from the naive expectation, but binoc's behavior is arguably correct. Worth a design note / possible doc clarification. |
| 🟠 missing feature | Fell back to opaque/byte-level handling where format-awareness was expected (JSON, XML, Excel, etc. treated as bytes). |
| 🔴 bug | Wrong, misleading, or nonsensical: false move/copy, mis-paired files, wrong counts, a claimed change that didn't happen, or a real change missed. |
| 💥 crash | Non-zero exit, panic, or traceback from the diff itself. |

**Anti-bias rule for judging.** You may read binoc's source or docs to explain
*why* a result happened (root cause), but never lower the bar to match what
binoc does. The verdict is set by the user-facing `expected`, not by binoc's
design rationale. "It's intentional" still counts as 🟠/🔵 if a reasonable user
would be surprised — note the intent, keep the finding.

Testing can be fanned out: batch scenarios across a few tester subagents
(full tool access) that each build, run, and return structured verdicts, then
synthesize. Single-threaded inline testing is fine for small runs.

## Report

Produce a Markdown report:

1. **Summary table** — every scenario: id, one-line change, verdict, severity.
2. **Findings** — for each non-✅ result, sorted by severity (💥, 🔴, 🟠, 🟡,
   🔵): the scenario, the exact command, what was expected, the verbatim binoc
   output, the verdict, and a minimal repro (inline the tiny snapshot files so a
   maintainer can paste-and-run, or point at the kept scratch path).
3. **Themes** — patterns across findings (e.g. "no structured-format support
   beyond tabular/text," "encoding changes always read as full rewrites,"
   "many-to-one merges reported as unrelated add/remove").

Keep the scratch root until the user has seen the report (mention its path), so
findings can be re-run. Don't add anything to `test-vectors/` automatically —
offer that as a follow-up for the scenarios worth hardening into real vectors.

## Parameters

- **Breadth**: number of generator subagents and scenarios per generator
  (default ~4 generators × ~8 scenarios).
- **Depth**: stdlib-only vs. bundled-plugin runs; whether to also probe with an
  obvious config.
- **Focus**: the user may pin a domain or change-type to concentrate the fuzzing
  (e.g. "encodings and line endings only"). Pass it as the seed; keep the
  briefing otherwise unchanged so naivety is preserved.
