# Benchmark method

The [report](README.md) compares Ferruddle with Defuddle 0.19.5 TypeScript
source under Vitest/Vite SSR and linkedom. It measures offline API work on
identical inputs. Network retrieval and browser rendering are outside its scope.

Run commands from the repository root. Reference preparation needs Python 3.11
or newer, Node.js and npm. The recorded measurements use Node v26.8.2 and
Rust 1.99.1; the crate's minimum Rust version is 1.88.

## Inputs and modes

Use all 208 non-math corpus pages, including LessWrong, with their exact HTML
strings and URLs: 1,372,181 HTML bytes. Exclude the 18 math cases from both
implementations. Each call parses a fresh DOM.

The corpus manifest SHA-256 is
`0e137fc415fce1832582b273e87bf2e64b1f93473d8b0ad29070fab40aa35dfa`.
The ordered `(id, url, inputSha256)` identity is
`e26a1dbbe0554fe280b9958642f8611822ea7dc593dae21f5ccaff66f5e0ab76`.
Each worker independently validates the selection and every input hash.

Measure HTML and HTML plus Markdown separately. Use equivalent default cleanup
options and disable fetch; TypeScript uses `useAsync:false`. The separate
226-case quality gate establishes semantic correctness. Benchmark fingerprints
establish each implementation's output stability.

## Prepare the reference

```sh
node tools/benchmark/prepare-reference.mjs
node tools/benchmark/benchmark.mjs --validate
node --test tools/benchmark/benchmark-checks.mjs
```

Preparation verifies every file in the [source archive](measured-source.tar.gz)
against its [manifest](measured-source.json). It reconstructs the reference under
ignored `.benchmarks/reference/` and installs locked npm dependencies with
lifecycle scripts disabled. This setup step needs network access if dependencies
are not cached. Timed extraction makes no network requests.

Preparation rejects a nonempty destination. To select another directory, pass
its path to `prepare-reference.mjs` and set `FERRUDDLE_REFERENCE_DIR` to that
path for measurement. Set `PYTHON` if the Python executable is not `python3`.
Normal Rust builds and tests do not need this reference or Node.

The coordinator checks reference hashes, invokes the archived Vitest worker and
passes it the same selection as the Rust worker. It records the reference and
Ferruddle source hashes separately.

## Warm timers

Each run starts fresh implementation processes in two orders:

| Measurement | Implementation order | Mode order |
| --- | --- | --- |
| 1 | Rust, TypeScript | HTML, HTML + Markdown |
| 2 | TypeScript, Rust | HTML + Markdown, HTML |

Each mode receives two whole-corpus warmups and seven measured rounds in each
process. Case order rotates each round. This gives 14 samples per page, mode
and implementation per run. Report each run separately.

Per-case timers include DOM parsing, extraction, metadata, word counting,
serialization and optional Markdown. Full-corpus timers also include timing and
collection bookkeeping. Imports, input loading, builds, output validation,
hashing and final result disposal occur outside warm timers. Rust DOM
destruction inside `extract` remains timed. V8 can collect garbage during
extraction or untimed validation; no forced GC is used.

Retain results until the corpus timer stops. Then validate HTML, Markdown,
title, author, site, publication date, word count and extractor type. Rust passes
its complete `Extraction` through `black_box` without traversing output inside
the timer. TypeScript checks every projected field against the pinned oracle.
Own output hashes must match across warmups, rounds, process orders and cold
calls. Native outputs must be nonempty, match the Markdown mode and contain no
unsupported-math warning.

## Cold timers

Use the 319-byte `issues--132-hero-class` input. Each implementation and mode
gets seven fresh processes, one call per process and no warmup. Alternate
implementation and mode order. Time the first parse/extract call after imports
and input validation. This includes lazy regex or JIT work and excludes process
launch and module transformation.

## Run a comparison

Use a working Rust toolchain on PATH. Build and test before timing, then run a
smoke check in a new output directory:

```sh
cargo build --release --locked --example benchmark
node tools/benchmark/benchmark.mjs .benchmarks/results/smoke 3 1
```

Discard the smoke timings. Finish other builds and tests, then wait for idle.
Start monitoring in a separate terminal before the measured command:

```sh
vmstat -w -t 5 > /tmp/ferruddle-benchmark-vmstat.log
```

Run the measurement:

```sh
node tools/benchmark/benchmark.mjs .benchmarks/results/run-1 7 2
```

Stop monitoring after measurement. Repeat with a new output directory and monitor
log. Keep each log with its results, exclude its first since-boot-average row
and record active swap IO. Record CPU frequency policy, memory, blocked tasks
and concurrent workloads; load average alone cannot establish an idle host.
Do not run other agents, builds or tests during timing.

## Statistics and evidence

Save warmups, raw samples, selections and output proofs. Retain source, harness,
input and executable hashes, lockfiles, dependency versions, compiler details,
configuration hashes, process order and host monitoring. Do not expose secret
environment values. The harness copies prebuilt executables into local `bin/`
directories and checks their hashes around each worker. Binaries are excluded
from version control and the crate package.

Report corpus median, upper quartile, minimum, maximum, sample count and
throughput. Report page median and p95 across the 208 page medians, plus each
page's median, upper quartile and range. Emit a sample p95 only with at least
20 observations. With 7 or 14 samples, call the maximum a maximum.

Supplementary native reports compare two archived Rust sources using the same
worker and separate Cargo targets. Their per-page flags use a 5% slowdown
threshold and a conservative threshold based on A/A variation. These flags are
descriptive, not significance tests. The live coordinator compares Ferruddle
with TypeScript; it does not build the native baseline.

Keep saved results and hashes immutable. Paths and runner scripts within raw
evidence identify the measured workspace; use the commands above for a new
measurement. The [evidence index](evidence.md) links the source archives and
independent statistical verifiers. Current-source verification permits explicit
crate/CLI naming, layout and documentation differences, while requiring the
same extraction code, dependencies and inputs.

## Update the pinned reference

Use a separate upstream checkout. Run its original suites and export a new
bundle there. Review HTML, metadata, Markdown structure, source preservation
and sanitization before replacing the pinned bundle. Preserve existing source
archives and measurement evidence. The Rust [quality contracts](../quality.md)
define acceptance; timing fingerprints alone cannot approve an output change.
