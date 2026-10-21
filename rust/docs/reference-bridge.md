# Rust parity bridge

Run from the repository root after `npm ci`. The exporter uses the upstream
fixtures, full math implementation, and comparison helpers. Normal `npm test`
discovery does not run these `*.oracle.ts` files.

```bash
port_oracle_parent="$(mktemp -d)"
TZ=UTC DEFUDDLE_ORACLE_DIR="$port_oracle_parent/linkedom" \
  npx vitest run --config scripts/porting/vitest.config.ts

# Optional second reference backend:
TZ=UTC DOM=jsdom DEFUDDLE_ORACLE_DIR="$port_oracle_parent/jsdom" \
  npx vitest run --config scripts/porting/vitest.config.ts
```

The output directory must not exist. Generation fails on missing or incorrect
upstream expectations and never updates them. It blocks injected/global fetch,
checks the existing Markdown/metadata goldens and optional HTML goldens, compares
synchronous extraction with the offline async path, and only then writes output.

Each case contains its HTML input, exact page URL, reference output, upstream
paths/hashes, and synchronous-mode differences. `manifest.json` identifies every
case by hash and records source/lockfile hashes and runtime versions. Keep the
bundle fixed while implementing the Rust port. Retain the upstream license when
copying it to a separate repository.

The strict diagnostic comparator accepts JSONL, one
object per line. Its stdout must contain only records; diagnostics go to stderr:

```json
{"id":"fixture-name","result":{"content":"<article>...</article>","contentMarkdown":"...","title":"...","author":"...","site":"...","published":"...","wordCount":12,"extractorType":null}}
```

`content` is always cleaned HTML in this protocol. Missing `extractorType` must
be represented as `null`. All eight fields are required; other fields are ignored.
Then compare:

```bash
TZ=UTC \
DEFUDDLE_ORACLE_DIR="$port_oracle_parent/linkedom" \
DEFUDDLE_RUST_RESULTS=/tmp/defuddle-rust-results.jsonl \
  npx vitest run --config scripts/porting/vitest.config.ts
```

The comparator rejects missing, unknown, or duplicate IDs and verifies bundle
hashes. HTML comparison uses upstream attribute-order normalization, retaining
attribute values and structure. Markdown comparison only trims trailing whitespace and
normalizes SVG attribute order outside code fences, as the upstream suite does.
Metadata, word count, and extractor type compare exactly. There are no parser
deviation exceptions in this comparator.

For independent Markdown tests, pass `expected.content` to the Rust converter and
compare with `expected.contentMarkdown`; this isolates conversion from extraction.
The exporter covers top-level fixture tests. The Rust crate adds focused inline
regressions and separate nested YouTube provenance/coverage.

The strict comparator records exact differences and is not the quality gate for
the implemented port. The accepted scope excludes math conversion and permits
reviewed semantic differences. See the [Rust library](../README.md) and its
[quality contract](quality.md). `cargo run --release --manifest-path
rust/Cargo.toml --example corpus -- /tmp/defuddle-rust-corpus` writes complete
per-case native results; the default Rust tests enforce the quality contract.

`node scripts/porting/benchmark.mjs OUTPUT_DIR 7 2` runs the reproducible offline
TS/Rust benchmark after `npm ci`. It saves raw per-case/per-round samples and
environment/source provenance. Math cases are excluded from both implementations.
Use a new empty output directory. Two passes reverse implementation and mode
order; each pass has two warmups and seven measured rounds by default. Output
validation and hashing happen after timers stop. Cold results measure the first
API call after imports, with process startup excluded.

`node --test scripts/porting/benchmark-checks.mjs` verifies harness planning and
statistics without measuring extraction. `node scripts/porting/benchmark.mjs
--validate` verifies the full input selection without calling either extractor.
See the [measurement method](benchmarks/methodology.md).

After that paired run, `benchmark-native-repeat.mjs MAIN_OUTPUT BASELINE_EXECUTABLE
BASELINE_CRATE` repeats the original and current native runtimes in alternating
order. Prepare the baseline crate and binary as described in the measurement
method. The command verifies original runtime hashes, identical worker code and
input contracts, and the current paired-run provenance. It refuses to overwrite
evidence and writes all four raw runs plus `native-repeat-summary.json`, including
per-page results and the previously flagged slow cases. Ratios and 5% flags are
descriptive measurements, not tests of statistical significance.
