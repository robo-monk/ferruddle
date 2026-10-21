# Development

Run from the repository root with Rust 1.88 or newer:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Normal Rust tests need no Node runtime or network once dependencies are cached.
They include all 226 pinned upstream HTML instances, readable native snapshots,
metadata/security/prose/structure checks and 18 original math-source contracts.
See [the quality contract](quality.md) before changing expectations.

The [Rust CI workflow](../.github/workflows/ci.yml) runs stable tests on
Linux, macOS and Windows, the minimum supported Rust version on Linux, and
formatting, Clippy, documentation and package checks. Platform jobs are distinct
from the local Linux verification recorded in [verification.json](verification.json).

## Source layout

| Location | Responsibility |
| --- | --- |
| `src/lib.rs`, `src/types.rs` | Public API and serialized types |
| `src/pipeline.rs`, `src/selection.rs` | Preparation, candidate attempts, retries and winner selection |
| `src/removals.rs`, `src/patterns.rs` | Clutter selectors, scores and repeated content patterns |
| `src/standardize.rs` and element modules | DOM normalization, rich code, images, tables, footnotes and callouts |
| `src/extractors/` | Offline site adapters |
| `src/metadata.rs` | Metadata and schema fallback |
| `src/markdown.rs` | Markdown rendering and safe retained HTML |
| `src/math.rs`, `src/safety.rs` | Unsupported-math detection/source retention and unconditional sanitization |
| `tests/internal/` | Crate-local tests of private implementation details |
| `tests/*.rs`, `tests/data/` | Public API regressions, original inputs and quality contracts |

Start extraction debugging with `Options { debug: true, ..Default::default() }`.
Inspect `debug.content_selector` and `debug.removals`. Disable individual removal
options to isolate content loss. Debug mode preserves diagnostic attributes and
skips some flattening; it is not the normal serialization contract.

## Corpus diagnostics

These commands report differences without updating contracts:

```sh
cargo test --lib tests::corpus::report_full_pipeline -- --ignored --nocapture
cargo run --release --example corpus -- /tmp/ferruddle-corpus
```

The first writes `/tmp/ferruddle-quality.json`. Ignored tests are explicit
diagnostic helpers, not skipped acceptance tests. Review visible semantic diffs
and the original input before changing any fingerprint.

The [pinned TypeScript comparison](reference-bridge.md) explains preparing the
archived reference in an ignored directory. Node is needed only for this optional
benchmark, not Rust development. Original fixtures and expectations stay fixed.

For timings, follow the [benchmark method](benchmarks/methodology.md). Build and
test before starting a measured window. Keep raw results immutable and use a new
directory for every run.
