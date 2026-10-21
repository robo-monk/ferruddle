# Schema preparation response to performance review R1

Implemented findings 1.2 and the schema search correction in 1.3 from
[hardening-opus-performance-r1.md](hardening-opus-performance-r1.md).

`Prepared::schema_root` is now a `OnceCell<Option<OriginalNode>>`. Preparation
selects the ordinary main root first and computes the schema root only if that
root is `body`. A later schema fallback initializes the same cache on demand.
Both successful matches and misses are cached. The prepared source remains
immutable, so cached IDs retain the existing clone-stability invariant.

The sanitized clone lookup, exclusion of unsafe roots/ancestors/descendant text,
document-order tie breaking, retry order and thresholds, and final sanitizer
are unchanged. In particular, explicit unsafe user selection still returns
empty output and cannot be replaced by a schema candidate.

The first schema paragraph is now trimmed **before** taking its first 100
characters, with the existing final trim retained. This matches
`src/defuddle.ts:714-715`. A new anonymous fixture has 160 leading spaces and a
short explicitly selected teaser. Recovery must select the eligible original
paragraph and preserve its authored `<strong>` markup. The recovered paragraph
is directly under `body`, keeping unrelated equal-word-count wrapper ties out
of this focused regression. No runtime tie-breaking rule was changed.

## Fail-before and focused checks

- The new operation-count test fails on eager preparation: an already adequate
  article with `articleBody` incurs **one** schema lookup when **zero** are
  needed. It passes with lazy preparation.
- The final leading-whitespace fixture fails against the lazy-only, pre-trim
  implementation: fallback returns plain schema text and loses the source
  `<strong>` markup. It passes after trim-order correction with the exact
  authored expected HTML and original paragraph debug selector.
- Pipeline unit tests pass **6/6**. They cover no unnecessary schema lookup,
  cache hits and misses, body-root cache initialization/reuse, clone-stable IDs,
  sanitized retry metrics, and shared preparation/rendering.
- Public `hardening_pipeline` regressions pass **10/10**, including the existing
  unsafe-schema fallback and explicit unsafe positional-selection cases.
- Targeted `rustfmt --check` passes. The root agent handles the final integrated
  suite, corpus gate, and Clippy checks.

The source/fixtures changed are `rust/src/pipeline.rs`,
`rust/tests/hardening_pipeline.rs`, and the new
`rust/tests/{fixtures,expected}/hardening/schema-leading-whitespace.html`.
No corpus snapshot, allowlist, upstream fixture, or benchmark harness changed.

## Complete corpus equivalence

Before editing, the current native source was copied into an isolated evidence
workspace. Every pinned upstream case was extracted twice: once with
`markdown:true`, and once with `markdown:true,debug:true`. Each of the 226 JSON
files contains both complete `Extraction` records, including HTML, Markdown,
all metadata/schema data, word count, extractor type, warnings, debug selector,
removals and errors. Thus each phase captures **452 complete extractions**.

Only `pipeline.rs` changed in the isolated source. This prevents concurrent
work by other agents from contaminating the comparison.

| Comparison | Cases / complete extractions | Byte-identical files | Changed cases |
| --- | --- | --- | --- |
| Eager preparation → lazy-only preparation | 226 / 452 | 226 | 0 |
| Lazy-only → trim-order correction | 226 / 452 | 226 | 0 |

The trim correction deliberately changes the new leading-whitespace regression;
none of the existing 226 corpus outputs changed. The upstream manifest remains
`0e137fc415fce1832582b273e87bf2e64b1f93473d8b0ad29070fab40aa35dfa`.

Evidence is preserved under
`/home/robomonk/.cache/defuddle-port-hardening/schema-r2/`:

- `verification.json`: complete per-file hashes, isolated source hashes, and
  both equivalence results.
- `before/`, `lazy-after/`, `trim-after/`: reviewable complete normal/debug JSON.
- `pipeline-before.rs`, `pipeline-lazy-only.rs`, `pipeline-trim.rs`: staged source.
- `lazy-before-test.log`: eager lookup count failure.
- `trim-lazy-before-final-fixture.log`: the final fixture's pre-trim failure.
- `source/examples/schema_dump.rs`: the public API capture program.

The isolated source used its own
`target-schema-r2-evidence` Cargo directory. Current tests used the current-only
target from `env.sh`; no baseline artifacts were built there. All commands used
the explicit working Rust 1.99.1 toolchain. These were output-equivalence and
functional checks; no extraction benchmarks or timings were collected.

Final formatted pipeline SHA-256:
`c4a24058422ba947d7a724579abd245ad9c0158169ca65e5c62a149265ee71f8`.
