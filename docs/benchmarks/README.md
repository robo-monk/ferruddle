# Benchmarks

Measured on 2026-10-21 (Europe/Athens) against Defuddle 0.19.5, revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. Both implementations processed
the same 208 pinned non-math pages, 1,372,181 HTML bytes and original URLs.
All 18 unsupported-math cases were excluded from both implementations. Fetch
was disabled. Quality was checked separately on all 226 cases.

Two complete monitored runs gave **7.29–7.48×** faster HTML extraction and
**7.38–7.61×** faster HTML plus Markdown extraction in Rust. The reference is
TypeScript source under Vitest/Vite SSR with linkedom.

| Run | Output | TypeScript corpus median | Rust corpus median | Speedup |
| --- | --- | ---: | ---: | ---: |
| 1 | HTML | 3,663.3 ms | 502.3 ms | 7.29× |
| 1 | HTML + Markdown | 3,925.2 ms | 515.7 ms | 7.61× |
| 2 | HTML | 3,866.7 ms | 516.8 ms | 7.48× |
| 2 | HTML + Markdown | 3,945.5 ms | 534.8 ms | 7.38× |

Each run uses two fresh-process passes. Pass 1 runs Rust then TypeScript, HTML
then Markdown. Pass 2 reverses implementation and output order. Every output
mode receives two whole-corpus warmups and seven measured rounds per pass:
14 samples per page/output in each run, 28 across the two runs. Each call parses
a fresh DOM. Case order rotates each round. Results are reported per run.

Warm timers include parsing, extraction, metadata, word counting, HTML
serialization and optional Markdown. Builds, imports, input loading, output
validation/hashing and final result disposal are outside them. Rust DOM
destruction inside `extract` is timed; V8 can collect DOM garbage during API
work or untimed validation. No forced GC is used. The release worker was
pre-built; every measured executable matches its recorded pre-build hash.

## Original/current native comparison

Both native versions used the same updated public-API benchmark worker,
Cargo profile, locked dependencies, compiler, inputs and options. Each run
executes original/current/current/original, reversing output order in pass 2.
Own outputs must stay stable. Changed original/current output proofs are listed
explicitly because the port also contains correctness fixes.

| Run | Output | Original Rust median | Current Rust median | Time reduction |
| --- | --- | ---: | ---: | ---: |
| 1 | HTML | 849.9 ms | 508.0 ms | 40.23% |
| 1 | HTML + Markdown | 869.7 ms | 533.5 ms | 38.66% |
| 2 | HTML | 898.7 ms | 544.3 ms | 39.44% |
| 2 | HTML + Markdown | 934.8 ms | 578.7 ms | 38.09% |

Individual pages vary. Run 1 had one page per output mode above 5% slower in
both native passes. Run 2 had two HTML pages and eight Markdown pages above
that threshold. Its more conservative A/A-noise flag marked Mastodon Markdown
(+0.209 ms) and ChatGPT thought-content Markdown (+0.042 ms). No page exceeded
5% in all four native passes across both runs. This does not establish an
absence of small regressions; the flags and A/A floor are descriptive and have
limited sensitivity. Full per-page ratios and the previously flagged cases
remain in the raw reports.

The archived initial results remain unchanged. Their timer method included
different checksum/disposal work, so the table above uses a fresh same-worker
original/current comparison.

## Cold API calls

Each sample is the first API call in a fresh process after imports and input IO.
The input is the original 319-byte `issues--132-hero-class` page. Each run has
seven samples per implementation/output. These measure one small page's cold
API cost; process launch and module import are excluded.

| Output | Rust median range across runs | TypeScript median range |
| --- | ---: | ---: |
| HTML | 17.59–17.74 ms | 57.05–57.19 ms |
| HTML + Markdown | 17.84–17.87 ms | 65.49–65.74 ms |

## Host conditions and limits

The host is an Intel Core i7-4870HQ laptop, eight logical CPUs, Linux
7.2.5-3-omarchy, `schedutil` governor, Node v26.8.2 and Rust 1.99.1. Rust used
thin LTO and one codegen unit. Other task agents, builds and tests had finished.

**The host was not fully isolated.** About 46 preexisting tasks were blocked in
the GPU driver, contributing to load averages around 48–49. The monitor shows
CPU idle medians of 86% and 85% during the two windows, including worker import
and validation phases. Existing swap allocation was about 9.2 GiB, and active
paging occurred:

| Run | Five-second samples | Swap-in | Swap-out | Maximum IO wait |
| --- | ---: | ---: | ---: | ---: |
| 1 | 47 | 0.70 MiB | 5.82 MiB | 1% |
| 2 | 48 | 25.73 MiB | 0.77 MiB | 0% |

The first vmstat row, which averages since boot, is excluded. Raw CPU/memory,
swap counters, blocked-task counts and every interval are retained. These are
repeatable observations of this host, with the recorded paging and frequency
variation. They are not isolated-host performance guarantees. The differing
per-page flags warrant caution about small changes.

## Evidence and reproduction

- [Run 1 summary](results/run-1/summary.json) and
  [run 2 summary](results/run-2/summary.json).
- [Cross-run source/output checks and per-page replication](results/run-1/replication-summary.json).
- [Run 1 native repeat](results/run-1/native-repeat-summary.json)
  and [run 2 native repeat](results/run-2/native-repeat-summary.json).
- [Run 1 host monitor](results/run-1/quiet-window.json) and
  [run 2 host monitor](results/run-2/quiet-window.json).
  Both result directories contain `vmstat.log`, pre-idle samples, cold samples,
  every raw warmup/measurement, source/input hashes and executable hashes.
- [Original native source archive](baseline/source.tar.gz)
  and its [file/hash manifest](baseline/source-manifest.json).
  This preserves the benchmark baseline independently of the local build cache.

To measure the current Ferruddle crate from the repository root:

```sh
node tools/benchmark/prepare-reference.mjs
cargo build --release --locked --example benchmark
# Wait for idle and start vmstat before measuring.
node tools/benchmark/benchmark.mjs .benchmarks/results/run-1 7 2
```

The [method](methodology.md) describes a discarded smoke run, monitoring and
replication. The pinned TypeScript project is reconstructed from the measured
source archive under `.benchmarks/reference/`; it is not application source.
The historical original/current native comparison above remains in the raw
results. Its original coordinator is preserved in the measured source archive.

Independent Python recomputation matched the speed ratios and regression flags
in both runs. The recomputation programs are included for replay (with failure exit codes for mismatches):

```bash
python3 docs/benchmarks/tools/verify-paired.py docs/benchmarks/results/run-1
python3 docs/benchmarks/tools/verify-native.py docs/benchmarks/results/run-1
```

Pass `docs/benchmarks/results/run-2` to the same programs to verify run 2. The
[quality gate](../quality.md) remains independent of timing proofs and does not
require byte-identical native/reference HTML or Markdown.

## Source identity

The [evidence manifest](evidence.json) checksums all unchanged raw results and
maps the former workspace paths to `run-1` and `run-2`. Original paths inside
raw JSON describe the measured workspace and have not been rewritten.
The [measured source archive](measured-source.tar.gz) and its
[manifest](measured-source.json) preserve all 323 measured source, harness and
input files. Local executable copies are omitted; both native source versions
can be rebuilt.

Measurements predate the rename to Ferruddle and move to the repository root.
The crate identity, CLI name, documentation and package metadata changed; the
extraction implementation and locked dependencies are unchanged. The verifier
normalizes only those explicit identity changes when comparing the current
source with the measured source. Verify that relationship, saved hashes, speed
ratios and regression flags from
the repository root with Python 3.11 or newer:

```sh
python3 docs/benchmarks/tools/verify.py
```

The compact [summary](summary.json) is derived from both final runs. Earlier
experiments live in the [archive](../archive/README.md) and are not combined
with these measurements.
