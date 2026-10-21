# Development

Run from the repository root with Rust 1.88 or newer:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Rust tests need no Node runtime or network once dependencies are cached.
They include all 226 pinned upstream HTML instances and 18 math-source contracts.
Read the [quality contracts](quality.md) before changing expectations.

The [CI workflow](../.github/workflows/ci.yml) defines stable tests on Linux,
macOS and Windows, minimum-version tests on Linux, formatting, Clippy,
documentation, package validation and tests of the unpacked crate.

## Source layout

| Location | Responsibility |
| --- | --- |
| `src/lib.rs`, `src/types.rs` | Public API and serialized types |
| `src/pipeline.rs`, `src/selection.rs` | Preparation, candidates, retries and winner selection |
| `src/removals.rs`, `src/patterns.rs` | Clutter selectors, scoring and repeated content patterns |
| `src/standardize.rs` and element modules | Code, images, tables, footnotes, callouts and DOM normalization |
| `src/dom.rs`, `src/dom/` | Traversal, mutation and selector helpers |
| `src/extractors/` | Offline site adapters |
| `src/metadata.rs` | Metadata and schema fallback |
| `src/markdown.rs` | Markdown and safe retained HTML |
| `src/math.rs`, `src/safety.rs` | Math detection, source retention and unconditional sanitization |
| `tests/internal/` | Crate-local tests of private implementation details |
| `tests/*.rs`, `tests/data/` | Public API regressions, original inputs and quality contracts |

## Extraction invariants

Preparation resolves shared metadata, URL context and main selection. Candidate
attempts clean independent DOM clones. `OriginalNode` relies on clone-stable arena
indices: select IDs after preprocessing, image removal and adapter mutations,
and keep the prepared DOM unchanged while attempts use them. Collect node IDs
before mutations that can invalidate traversal.

Footnote and callout preparation precedes clutter removal. Sanitization runs
before candidate word counts and after transforms and URL resolution.
Determine unsafe root identity before retagging. Sanitize every fallback,
adapter and selected-root path, including inert templates and foreign namespaces.
Use DOM setters or escape page values when constructing HTML.

Keep retry thresholds and winner comparisons in `pipeline.rs`. Hidden retries
search body descendants and retain the first equal-sized candidate in document
order. Their length comparison uses sanitized, trimmed HTML. Only the winning
candidate renders Markdown.

Render each inline child once and retain cached lookahead results. Preserve
literal page text when Markdown is rendered into HTML, including raw HTML blocks,
attributes and table code. Math-source preservation must not introduce formula
conversion or executable markup.

## Debugging and diagnostics

Start with `Options { debug: true, ..Default::default() }`. Inspect
`debug.content_selector` and `debug.removals`. Disable individual removal options
to isolate content loss. Debug mode retains diagnostic attributes and skips some
flattening; its HTML can differ from normal output.

These commands report differences without updating contracts:

```sh
cargo test --lib tests::corpus::report_full_pipeline -- --ignored --nocapture
cargo run --release --example corpus -- /tmp/ferruddle-corpus
```

The first writes `/tmp/ferruddle-quality.json`. Ignored tests are explicit
helpers, separate from acceptance tests. Review the original input and visible
semantic diffs before changing fingerprints.

Node is needed only for the optional [TypeScript benchmark](benchmarks/methodology.md).
Build and test before measurement. Use new output directories, separate Cargo
targets for distinct native versions and immutable raw results.
