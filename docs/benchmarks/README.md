# Benchmarks

The measured Rust speedup is **7.29–7.48×** for HTML extraction and
**7.38–7.61×** for HTML plus Markdown across 208 pinned non-math pages.
These are measurements from two runs on one laptop with active paging.
They are not isolated-host performance guarantees.

The reference is Defuddle 0.19.5, revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`, running as TypeScript source
under Vitest/Vite SSR with linkedom. Measurements date from 2026-10-21.

## Warm extraction

| Run | Output | TypeScript corpus median | Rust corpus median | Speedup |
| --- | --- | ---: | ---: | ---: |
| 1 | HTML | 3,663.3 ms | 502.3 ms | 7.29× |
| 1 | HTML + Markdown | 3,925.2 ms | 515.7 ms | 7.61× |
| 2 | HTML | 3,866.7 ms | 516.8 ms | 7.48× |
| 2 | HTML + Markdown | 3,945.5 ms | 534.8 ms | 7.38× |

Both implementations process identical HTML strings and URLs: 208 pages and
1,372,181 HTML bytes. All 18 unsupported-math cases are excluded from both.
Fetch is disabled. The separate [quality gate](../quality.md) covers all 226 cases.

Each run uses two fresh-process measurements with reversed implementation and
output order. Each measurement has two whole-corpus warmups and seven timed
rounds per mode: 14 samples per page, mode and implementation in each run.
Every call parses a fresh DOM.

Timers include parsing, extraction, metadata, word counting, HTML serialization
and optional Markdown. Builds, imports, input loading, output checks and final
result disposal are outside the timers. Rust DOM destruction within `extract`
is timed. V8 can collect garbage during API work or untimed validation.
No forced GC is used.

## Cold API calls

Each sample is the first call in a fresh process after imports and input loading.
The input is the 319-byte `issues--132-hero-class` page. Each run has seven
samples per implementation and mode. Process launch and imports are excluded.

| Output | Rust median range across runs | TypeScript median range |
| --- | ---: | ---: |
| HTML | 17.59–17.74 ms | 57.05–57.19 ms |
| HTML + Markdown | 17.84–17.87 ms | 65.49–65.74 ms |

These figures describe cold API cost for one small page.

## Host limits

The host uses an Intel Core i7-4870HQ, eight logical CPUs, Linux
7.2.5-3-omarchy and the `schedutil` governor. The runtimes are Node v26.8.2
and Rust 1.99.1. Rust uses thin LTO and one codegen unit.

About 46 tasks blocked in the GPU driver contribute to load averages of 48–49.
Median CPU idle is 86% and 85% across the two monitoring windows, including
imports and validation. Allocated swap is about 9.2 GiB. Active paging occurs
in both runs:

| Run | Five-second intervals | Swap-in | Swap-out | Maximum IO wait |
| --- | ---: | ---: | ---: | ---: |
| 1 | 47 | 0.70 MiB | 5.82 MiB | 1% |
| 2 | 48 | 25.73 MiB | 0.77 MiB | 0% |

The interval statistics exclude the first vmstat row, which averages since boot.
Paging, frequency variation and the small sample count limit interpretation of
small per-page differences. Aggregate speedups do not establish an absence of
individual regressions.

## Reproduce and inspect

Follow the [method](methodology.md) to prepare the pinned reference and run a
comparison. The [evidence index](evidence.md) links every raw result, source
archive, host log and verification script. The compact
[summary](summary.json) contains the table's exact values.

Check saved hashes, source identity, statistics and output stability without
building or accessing the network:

```sh
python3 docs/benchmarks/tools/verify.py
```
