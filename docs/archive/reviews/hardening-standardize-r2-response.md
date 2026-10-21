# Standardizer performance response to Opus section 1.1

The section 1.1 reorderings and single-walk optimization are implemented with
unchanged F2 source-boundary semantics. Section 1.3 allocation and descendant
short-circuit improvements are also implemented. No snapshot, contract,
allowlist, TypeScript source, or upstream fixture was changed. No benchmark or
timing claim was produced.

## Changes and equivalence

- The NBSP pass checks the text for an NBSP before asking whether it is protected.
- Empty-element removal checks allowed tags, child kinds, and empty text before
  the protection query.
- The final punctuation pass asks for protection only when punctuation cleanup
  would change text.
- Wrapper preservation checks the fixed preserved-tag list before protection.
- `protected()` performs one ancestor walk with precompiled thread-local
  `dom_query::Matcher` values. It does not hash the long source-selector string
  on each call. It preserves the exact former expression:
  `closest(pre,code,svg) || closest(SOURCE_SELECTOR).is_some_and(is_math_source)`.
  In particular, only the nearest source-selector match is accepted by the
  source predicate; if that match fails, the walk continues only for code/SVG
  protection. No predicate/selector equivalence assumption was used to remove
  the matcher, and no F2 detection or retention boundary was broadened.
- `math::has_source_descendant` uses the same precompiled source matcher and
  final predicate as the former collected selector results. It stops at the
  first accepted descendant and excludes the root and inert template fragments.
  Hidden removal and meaningful-content detection now use this helper.
- Source class comparisons use `eq_ignore_ascii_case` without allocating a
  lowercase class string. Broad detection prefixes and control-class suffixes
  use bounded ASCII byte comparisons. These preserve `to_ascii_lowercase`
  semantics, including non-ASCII bytes and short strings. MathJax ignore/process
  classes remain excluded from warning evidence and source boundaries.

The optional rewrite of the first whitespace text loop was not adopted: the
review explicitly calls for measurement before adding its extra protected-code
text transformations. The review's 57% fewer protection calls describes its
operation-count probe; this response does not claim a measured timing gain.

## Complete-output equivalence proof

A private source-frozen crate was created before the changes at:

`/home/robomonk/.cache/defuddle-port-hardening/perf-standardize-r2-source`

It contains the 30 source/Cargo inputs from the current runtime, verified not to
have changed while copying, and a diagnostic example that serializes the full
public extraction result for every pinned case in default and debug modes,
with Markdown enabled. It uses the immutable upstream fixture directory. The
build target is dedicated:

`/home/robomonk/.cache/defuddle-port-hardening/target-baseline-standardize-r2`

After recording the baseline, only these files were transplanted into that same
frozen source crate:

- `src/math.rs`
- `src/removals.rs`
- `src/standardize.rs`

The source hash comparison confirms every other source/Cargo file stayed fixed.
This isolates the pure performance edits from concurrent Markdown/pipeline work.
The example was rebuilt and all results were recorded again.

**All 452 complete results are byte-identical:** 226 cases in each of default
and debug extraction. The comparison includes HTML, Markdown, every metadata
field, schema data, word counts, warnings, extractor type, content selector, and
ordered debug removal records. Both pretty-printed JSON files have SHA-256:

`7f82d184dd565183f0fe3d9f6652c868e08ae5dd062705e38a4b50f894051d2d`

Readable outputs and machine-readable evidence are retained under the cache:

- `perf-standardize-r2-before.json`
- `perf-standardize-r2-after.json`
- `perf-standardize-r2-equivalence.json`
- `perf-standardize-r2-source/before-source-hashes.json`
- `perf-standardize-r2-source/after-source-hashes.json`
- `perf-standardize-r2-source/examples/perf_equivalence.rs`

## Focused checks on the working tree

All commands sourced
`/home/robomonk/.cache/defuddle-port-hardening/env.sh`; normal tests used the
current-only target. The isolated proof used the dedicated target above.

- `cargo test --manifest-path rust/Cargo.toml --test math_safety -- --nocapture`:
  **8 passed**, including F2 controls/prose wrappers and all 18 formula-source
  contracts in default extraction, debug extraction, and standalone conversion.
- `cargo test --manifest-path rust/Cargo.toml --lib math::tests -- --nocapture`:
  **5 passed**. New equivalence oracles compare the short-circuit descendant
  query with collected selector results and bounded ASCII comparisons with the
  former lowercase behavior, including Unicode boundary cases.
- `cargo test --manifest-path rust/Cargo.toml --lib standardize -- --nocapture`:
  **20 passed**. The new ancestor-walk oracle compares protection with the
  former two closest queries across code, SVG/foreign content, source roots,
  explicit holders, ignored processing controls, prose, and template fragments.

Final integrated corpus fingerprints, full-suite checks, and any later benchmark
remain with the coordinating worker.
