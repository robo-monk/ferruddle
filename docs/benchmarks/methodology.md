# Benchmark method

The [final report](README.md) compares Rust with the pinned Defuddle 0.19.5
TypeScript source under Vitest/Vite SSR and linkedom. It measures offline API
work on identical inputs, not network or browser rendering.

## Inputs and modes

Both implementations process all 208 non-math corpus pages, including LessWrong,
with their exact original strings and URLs: 1,372,181 HTML bytes. The 18 math
cases are excluded from both implementations. Each call parses a fresh DOM.

The manifest SHA-256 is
`0e137fc415fce1832582b273e87bf2e64b1f93473d8b0ad29070fab40aa35dfa`.
The ordered `(id, url, inputSha256)` identity is
`e26a1dbbe0554fe280b9958642f8611822ea7dc593dae21f5ccaff66f5e0ab76`.
Each worker independently validates this selection and every input hash.

Measure HTML and HTML plus Markdown separately, with equivalent default cleanup
options and fetch disabled. TypeScript uses `useAsync:false`. The separate
226-case quality gate establishes semantic correctness; benchmark fingerprints
only establish each implementation's own output stability.

## Warm timers

Each run has two fresh-process passes:

| Pass | Implementation order | Mode order |
| --- | --- | --- |
| 1 | Rust, TypeScript | HTML, HTML + Markdown |
| 2 | TypeScript, Rust | HTML + Markdown, HTML |

Each mode/pass has two whole-corpus warmups and seven measured rounds. Rotate
case order every round. This yields 14 samples per page/mode/implementation per
run. Report separate runs rather than pooling different host windows.

Per-case timers surround DOM parsing, extraction, metadata, counting and
serialization, plus Markdown when requested. Full-corpus timers additionally
include the small timing/collection bookkeeping. Imports, input IO, compilation,
output validation/hashing and final result disposal occur outside warm timers.
Rust DOM destruction inside `extract` remains timed. V8 can collect garbage
during extraction or untimed validation; no forced GC is used.

Retain all results until the full-corpus timer stops, then validate the complete
eight-field projection: HTML, Markdown, title, author, site, publication date,
word count and extractor type. Rust passes its complete `Extraction` through
`black_box` without traversing output in the timed loop. TypeScript compares
every projected field against the pinned oracle. Own output hashes must match
across warmups, rounds, passes and cold calls. Native outputs must be nonempty,
have the requested Markdown mode and contain no unsupported-math warning.

## Cold timers

Use the original 319-byte `issues--132-hero-class` input. Each implementation/mode
gets seven fresh processes, with one call and no warmup per process. Alternate
implementation and mode order. Time the first parse/extract call after imports
and input validation. These samples measure cold API work, including lazy regex
or JIT work, not process launch or module transformation.

## Run a new comparison

Prepare the archived TypeScript reference in its isolated directory. Use a
working Rust toolchain on PATH and a new empty output directory:

```sh
node tools/benchmark/prepare-reference.mjs
cargo build --release --locked --example benchmark
node --test tools/benchmark/benchmark-checks.mjs
node tools/benchmark/benchmark.mjs --validate
# Discard this smoke run's timings.
node tools/benchmark/benchmark.mjs .benchmarks/results/smoke 3 1
```

Finish all tests, builds and reviews, then wait for idle. Start host monitoring
in another terminal before the measured command, and stop it afterward:

```sh
vmstat -w -t 5 > /tmp/ferruddle-benchmark-vmstat.log
```

```sh
node tools/benchmark/benchmark.mjs .benchmarks/results/run-1 7 2
```

Repeat in a new directory for replication. Keep the monitor log with the results,
exclude its first since-boot-average row from interval statistics, and record
active swap IO. Do not run other task agents, builds or tests during timing.
The final report records existing GPU-driver waits, CPU idle, active paging and
frequency policy; load average alone cannot establish an idle machine.

## Historical original/current native comparison

The final report also retains a development comparison of the original and
optimized Rust implementations, using the same updated worker and separate Cargo
targets. Both source versions and the exact original coordinator are preserved
in the archives. These measurements predate Ferruddle's repository migration;
they are retained evidence rather than a second live benchmark workflow.

The current maintainer command measures the current Ferruddle crate against the
pinned TypeScript reference. It does not compile a Rust predecessor or require
another maintained application tree.

## Evidence and statistics

Keep every warmup, raw measurement, cold sample, selection and output proof.
Save source/harness/input hashes, lockfiles and dependency versions, compiler
details, relevant configuration hashes, governor, process order and environment.
Do not expose secret environment values. The harness copies pre-built executables
to local `bin/` directories and checks their hashes around every worker. Binaries
are rebuildable and excluded from version control and the public crate.

Report corpus median, upper quartile, min/max, sample count and throughput;
page median/p95 across the 208 page medians; and per-page median, upper quartile
and min/max. Emit sample p95 only for at least 20 observations. At 7 or 14
samples, report the maximum by that name instead of calling it tail latency.

The native comparison reports current pass-to-pass fold differences and marks
cases slower in both passes by more than 5%, plus a conservative flag above the
larger of 5% and the A/A fold-difference p95. These are descriptive flags, not
significance tests. A faster aggregate and an absent flag do not establish an
absence of individual regressions.

Saved results are immutable. [The verifier](tools/verify.py) validates the final
evidence and independently recomputes its statistics. The measured source archive
preserves the exact original harness; the current coordinator builds Ferruddle at the root and runs the pinned
TypeScript worker in an ignored reference directory. Historical JSON paths remain unchanged for provenance.
