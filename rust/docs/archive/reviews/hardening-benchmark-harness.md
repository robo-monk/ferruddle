# Paired benchmark harness

The benchmark harness is prepared for the final source-stable run. No extraction
timings were collected while preparing it. Existing `2026-10-20` evidence and
review performance artifacts remain untouched.

## Reproduction

After tests, review, and source edits have finished, use the working toolchain
from the root agent's environment and a **new empty** output directory:

```bash
source /home/robomonk/.cache/defuddle-port-hardening/env.sh
node scripts/porting/benchmark.mjs rust/benchmarks/after-hardening 7 2
```

The existing positional CLI remains `OUTPUT_DIR ROUNDS WARMUPS`. The default is
seven measured rounds after two complete warmup rounds. Nonempty output
folders are rejected so historical evidence cannot be overwritten accidentally.
Do not run builds, tests, reviews, or other heavy work during this command.

Cheap checks that do not call an extractor:

```bash
node scripts/porting/benchmark.mjs --validate
node --test scripts/porting/benchmark-checks.mjs
```

Both workers additionally support input validation without extraction:
`benchmark SELECTION OUTPUT --validate-only` for the native example and
`DEFUDDLE_BENCH_VALIDATE_ONLY=1` for the TypeScript Vitest worker. Selection files
must use schema version 2; older saved results are historical artifacts, not
inputs for the new workers.

## Warm measurement

The full 208-case non-math corpus remains in both implementations, including
LessWrong. This is the same 1,372,181 bytes of original HTML, with the original
URLs, manifest order, and 18 explicit math exclusions. The manifest hash is
`0e137fc415fce1832582b273e87bf2e64b1f93473d8b0ad29070fab40aa35dfa`.
The ordered `(id, url, inputSha256)` identity hash is
`e26a1dbbe0554fe280b9958642f8611822ea7dc593dae21f5ccaff66f5e0ab76`.

The coordinator executes two sequential passes, using a fresh process for each
implementation in each pass:

| Pass | Implementation order | Mode order within each process |
| --- | --- | --- |
| 1 | Rust, TypeScript | HTML, HTML + Markdown |
| 2 | TypeScript, Rust | HTML + Markdown, HTML |

Each mode gets its own two warmups and seven measured rounds in each pass.
That yields 14 measured full-corpus samples and 14 samples per page/mode for
each implementation. Within every round, page order rotates by the round index;
the raw record retains that offset. This balances implementation and mode
ordering without changing which pages are measured.

Input file loading, file/input/URL hash validation, imports, Rust compilation,
and process startup all finish before the extraction timers start. Each native
call parses its input into a new DOM; each TypeScript call executes
`parseDocument(inputHtml, url)` inside its timer, using production linkedom and
`useAsync:false`. Both injected and global TypeScript fetches are rejected.

There are two reported timing views:

- Full-corpus wall time surrounds the complete 208-case round. It includes
  extraction, DOM parsing, metadata, HTML serialization, optional Markdown,
  and the small cost of per-case timer calls and collecting results.
- Per-case time surrounds only the parse/extract API call. The report also
  gives the median of per-round sums of these times as a secondary measure.

Every round retains its result objects until its wall timer stops. Only then
are outputs validated, projected, hashed, and released. There is no checksum or
output traversal inside either timer. Rust result destruction is outside the
round timer. Node's normal garbage collection can still occur during API work;
no forced GC or runtime-specific cleanup is hidden in the comparison. This
method differs from the historical harness, which included checksum bookkeeping
and Rust result destruction inside the full-corpus timer. Keep old results as
historical context; do not present them as an otherwise identical paired rerun.

## Output validation and raw evidence

Every output from every warmup and measured round must be nonempty and have a
stable SHA-256 over the full shared eight-field projection: HTML, Markdown,
title, author, site, published, word count, and extractor type. Output walks and
reference comparisons happen outside all timers. The native hot path passes the
complete returned `Extraction` through `black_box`, preventing result elimination
without traversing output.

TypeScript checks projected fields against the pinned upstream oracle, allowing
only its explicitly recorded synchronous-difference fields (the current
manifest records none). Native output must have the requested Markdown mode
and no math warnings. Each implementation's complete output fingerprint must
remain identical across rounds, both passes, and cold/warm calls. The separate
native corpus quality gate establishes semantic correctness; this harness does
not bless or update quality expectations or assume native/TypeScript outputs
are byte-identical.

Both workers independently verify the manifest, math exclusion set, complete
ordered 208-case selection, every case file hash, input HTML hash, and URL.
Their results echo the verified identity. The coordinator rejects mismatched
sample counts/order, missing cases, malformed output proofs, and disagreements
between raw samples and per-case/full-corpus arrays.

The output directory retains:

- `selection.json`, the complete selected input identities and exclusions.
- `rust-pass-{1,2}.json` and `typescript-pass-{1,2}.json`, their exact per-run
  selections, all per-case/full-corpus measurements, and **all warmup samples**.
- `rust.json` and `typescript.json`, combined 14-sample warm results retaining
  pass identity on raw rounds and per-case output fingerprints.
- `cold.json`, individual cold worker files/selections, and raw cold samples.
- `provenance.json` and `summary.json`, source/harness/input hashes, dependency
  versions, environment hash, Rust executable hash, process execution order,
  process timestamps, and load averages before/after each process.
- `bin/defuddle-benchmark`, a private copy of the built executable. Only this
  copy is run; its SHA-256 is rechecked before and after every worker. Binary
  copies stay local and are ignored by Git.

Source provenance includes all Rust runtime files, Cargo files, TypeScript
runtime files, package files, shared test parsing helpers, Vitest configuration,
all files under `scripts/porting` (including the new shared helper and harness
tests), `tsconfig.json`, the installed `node_modules/.package-lock.json`, existing
Cargo config/toolchain files from the crate upward and in Cargo home, the native
benchmark example, and this method report. It records `rustc -vV`, the CPU
frequency governor when available, and hashes of Cargo/Rust environment override
values; names suggesting secrets are excluded. Source and input hashes are captured
before compilation and checked after compilation and around every worker. Any
change aborts the run. No baseline build is invoked by the harness.

The reference is TypeScript source under Vitest/Vite SSR and linkedom, rather
than the distributed `dist/node.js` bundle. Rust DOM destruction within
`extract` is timed; V8 can collect DOM garbage during the API call or the
untimed validation allocations. No forced GC is used. Pre-build the release
worker before timing so the harness build is a no-op, and run a discarded
three-round smoke benchmark before the quiet measurement window.

## Cold first-call measurement

Cold measurements use `issues--132-hero-class`, the pinned 319-byte real corpus
page used by the performance review. For every implementation/mode, the
coordinator starts seven separate processes by default; each performs exactly
one extraction and no warmup. Implementation and mode order alternate between
sample groups. All cold results must match that implementation's warm output.

The native executable starts, loads/validates its inputs, then times its first
`extract` call. The TypeScript process imports/transforms the source through
Vitest, loads/validates its input, then times its first `parseDocument`/Defuddle
call. Imports and module evaluation are excluded on both sides. These samples
measure cold **API extraction** after imports, including lazy regex/JIT work
triggered by that first call. They do not measure CLI launch latency or total
application startup; Vitest startup and transformation are specifically outside
the timer. Cold data uses separate processes and cannot warm the warm-run
workers.

## Repeating prior native per-case regression claims

Use the same updated worker for both native versions. Make a new benchmark-only
fork of the cached baseline; do not edit the preserved baseline itself. Copy
only the updated example and current exclusion-contract file into that fork.
The exclusion file supplies the pinned IDs/hash, not runtime behavior. Keep the
original baseline runtime source and original upstream inputs unchanged and
record their hashes. Build this fork before any timed work, using a separate
Cargo target to avoid the previously observed same-package artifact collision:

```bash
source /home/robomonk/.cache/defuddle-port-hardening/env.sh
cp -a /home/robomonk/.cache/defuddle-port-hardening/baseline /home/robomonk/.cache/defuddle-port-hardening/benchmark-baseline-worker
cp rust/examples/benchmark.rs /home/robomonk/.cache/defuddle-port-hardening/benchmark-baseline-worker/examples/benchmark.rs
cp rust/tests/data/quality/math.json /home/robomonk/.cache/defuddle-port-hardening/benchmark-baseline-worker/tests/data/quality/math.json
CARGO_TARGET_DIR=/home/robomonk/.cache/defuddle-port-hardening/target-benchmark-baseline cargo build --release --locked --manifest-path /home/robomonk/.cache/defuddle-port-hardening/benchmark-baseline-worker/Cargo.toml --example benchmark
```

Choose a fresh fork destination for this preparation. The updated worker uses
only the existing public `extract`/`Options`/`Extraction` API, which is compatible
with the cached baseline. Do not run baseline quality tests against the copied
new exclusion-contract contents: this fork is solely for benchmarking.

After the main paired run creates its schema-2 selection files, run the automated
native repeat. Both versions process all 208 pages. The driver reverses native
and mode order on the second pass, validates every raw run with the same shared
functions, checks self-stable output proofs, and computes per-case statistics:

```bash
bench_base=/home/robomonk/.cache/defuddle-port-hardening/target-benchmark-baseline/release/examples/benchmark
bench_output=rust/benchmarks/after-hardening
node scripts/porting/benchmark-native-repeat.mjs "$bench_output" "$bench_base" /home/robomonk/.cache/defuddle-port-hardening/benchmark-baseline-worker
```

Run this quietly after both binaries have finished building. The driver obtains
the current copied binary from `summary.json` and copies the baseline into the
same local `bin` directory. It verifies both executable hashes around each run,
the original baseline runtime against the historical source hashes, the shared
worker and exclusion file, and current source/input provenance. It writes four
raw runs and `native-repeat-summary.json`, with pooled and per-pass medians,
upper quartiles and maxima. Output proof differences are listed explicitly.

Current pass 1 versus pass 2 supplies a descriptive A/A noise distribution over
208 page-median fold differences. The repeat reports cases slower in both passes
by more than the larger of 5% and that distribution's p95. This flag is not a
statistical significance test or proof that no regression exists.

The exact cases/modes that need attention are already enumerated in
`rust/reviews/performance-evidence/verification.json` under
`benchmark.casesOver5PercentSlowerInAnyRun`: Mastodon, Hacker News, old Reddit,
X articles, ChatGPT thought content, a GitHub pull request, and Gmail. This
procedure rechecks those individual observations without removing LessWrong or
any other case from either measured corpus. If they remain noisy or cross the
comparison boundary in only one pass, repeat the alternating sequence; do not
infer absence of individual regressions from a faster batch median.

## Reporting and limits

`summary.json` reports full-corpus median, upper quartile, min/max, sample count
and throughput; median and p95 across the 208 page medians; and each page's own
median, upper quartile and min/max. It emits sample p95 only with at least 20
observations. At the default 7/14 samples, nearest-rank p95 equals the maximum,
so the maximum is named explicitly instead of being labeled tail latency.
It includes the aggregate TypeScript/Rust ratio, median page ratio,
combined-median slower cases, slower cases in either pass, and full per-pass
statistics. A faster aggregate is never interpreted as proof that no individual
case was slower. Small differences remain descriptive observations; inspect the
raw samples and repeat noisy cases before making regression claims.

This preparation passed seven Node tests covering alternating schedules,
HTML/URL/exclusion provenance, source-change rejection, sample dimensions/raw
agreement, cross-pass output stability, and arithmetic that reports slower
individual pages despite a faster aggregate, plus small-sample statistic labels. These use arbitrary values to test
reporting logic, with no fabricated performance assertions. `--validate`
verified all 208 real inputs without calling extraction. `cargo check --example
benchmark --locked` passed, the native `--validate-only` worker loaded and
validated all 208 strings/URLs successfully, and the TypeScript validation-only
worker passed through the real Vitest configuration. Formatting and
`cargo clippy --example benchmark --locked -- -D warnings` also passed with the
working toolchain on PATH and explicit RUSTC/RUSTDOC.

After the review follow-up, the coordinating agent ran a complete discarded
smoke benchmark (three measured rounds, one warmup): both 208-page passes,
all cold workers and output checks, followed by the validated four-run native
repeat. All commands exited zero. Smoke evidence stays in the local cache;
its timings are not published as final measurements. The release worker was
pre-built. The final run waits for independent review and a quiet window.

The R2 performance review requires another complete smoke after the final
renderer/hidden-retry fixes. Record the rebuilt binary hash before that smoke
and require the final copied binary to match it. Run the paired benchmark and
native repeat consecutively, editing neither source nor this method report in
between. Keep `vmstat -w -t 5` running to a separate file throughout, then attach
the raw monitor log and before/after CPU, memory, swap, and blocked-task counts.
Load average alone is insufficient on this machine: the review observed about
46 tasks blocked in the GPU driver while the CPU was idle. Its heavy existing
swap allocation must be distinguished from active swap IO during measurement.
Document the actual monitored conditions and any resulting limits.
