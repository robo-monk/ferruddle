# Benchmark evidence

The [report](README.md) interprets the results. The [method](methodology.md)
describes timing boundaries and reproduction. Files listed below are measured
data and executable provenance; preserve their bytes and recorded hashes.
Workspace paths inside saved data identify the measurement source.

## Source and verification

| File | Purpose |
| --- | --- |
| [Compact summary](summary.json) | Exact report values and limits |
| [Evidence manifest](evidence.json) | Raw file hashes and path mapping |
| [Measured source manifest](measured-source.json) | File hashes and source archive identity |
| [Measured source archive](measured-source.tar.gz) | Pinned reference, extraction code, harness and inputs |
| [Native baseline manifest](baseline/source-manifest.json) | Baseline source hashes |
| [Native baseline archive](baseline/source.tar.gz) | Rebuildable baseline source |
| [Native baseline contract](baseline/contract.json) | Runtime identity and per-page watchlist |
| [Evidence verifier](tools/verify.py) | Hashes, archives, current-source relationship and statistics |
| [Paired verifier](tools/verify-paired.py) | Independent Rust/TypeScript recomputation |
| [Native verifier](tools/verify-native.py) | Independent native comparison recomputation |

Run the full verifier from the repository root:

```sh
python3 docs/benchmarks/tools/verify.py
```

For an individual report, run either statistical verifier with
`docs/benchmarks/results/run-1` or `docs/benchmarks/results/run-2` as its argument.
These checks need Python 3.11 or newer and no network or build.

## Result files

`summary.json`, `rust.json` and `typescript.json` contain paired statistics and
per-page output proofs. `cold.json` collects first-call samples. The native
reports compare the two archived Rust implementations; their statistics are
separate from the Rust/TypeScript results. Host conditions are in
`quiet-window.json`, `vmstat.log` and `preidle.log`.

The saved `run-quiet-bench.py` files identify the recorded monitoring workflow.
Their absolute paths refer to the measured host. Use the current
[measurement commands](methodology.md#run-a-comparison) to collect new results.

### Summaries and provenance

| File | Run 1 | Run 2 |
| --- | --- | --- |
| `cold.json` | [run-1](results/run-1/cold.json) | [run-2](results/run-2/cold.json) |
| `independent-recomputation.log` | [run-1](results/run-1/independent-recomputation.log) | [run-2](results/run-2/independent-recomputation.log) |
| `native-repeat-baseline-1.json` | [run-1](results/run-1/native-repeat-baseline-1.json) | [run-2](results/run-2/native-repeat-baseline-1.json) |
| `native-repeat-baseline-2.json` | [run-1](results/run-1/native-repeat-baseline-2.json) | [run-2](results/run-2/native-repeat-baseline-2.json) |
| `native-repeat-current-1.json` | [run-1](results/run-1/native-repeat-current-1.json) | [run-2](results/run-2/native-repeat-current-1.json) |
| `native-repeat-current-2.json` | [run-1](results/run-1/native-repeat-current-2.json) | [run-2](results/run-2/native-repeat-current-2.json) |
| `native-repeat-summary.json` | [run-1](results/run-1/native-repeat-summary.json) | [run-2](results/run-2/native-repeat-summary.json) |
| `preidle.log` | [run-1](results/run-1/preidle.log) | [run-2](results/run-2/preidle.log) |
| `provenance.json` | [run-1](results/run-1/provenance.json) | [run-2](results/run-2/provenance.json) |
| `quiet-window.json` | [run-1](results/run-1/quiet-window.json) | [run-2](results/run-2/quiet-window.json) |
| `replication-summary.json` | [run-1](results/run-1/replication-summary.json) | — |
| `run-quiet-bench.py` | [run-1](results/run-1/run-quiet-bench.py) | [run-2](results/run-2/run-quiet-bench.py) |
| `rust.json` | [run-1](results/run-1/rust.json) | [run-2](results/run-2/rust.json) |
| `selection.json` | [run-1](results/run-1/selection.json) | [run-2](results/run-2/selection.json) |
| `summary.json` | [run-1](results/run-1/summary.json) | [run-2](results/run-2/summary.json) |
| `typescript.json` | [run-1](results/run-1/typescript.json) | [run-2](results/run-2/typescript.json) |
| `vmstat.log` | [run-1](results/run-1/vmstat.log) | [run-2](results/run-2/vmstat.log) |

### Warm samples

| File | Run 1 | Run 2 |
| --- | --- | --- |
| `rust-pass-1-selection.json` | [run-1](results/run-1/rust-pass-1-selection.json) | [run-2](results/run-2/rust-pass-1-selection.json) |
| `rust-pass-1.json` | [run-1](results/run-1/rust-pass-1.json) | [run-2](results/run-2/rust-pass-1.json) |
| `rust-pass-2-selection.json` | [run-1](results/run-1/rust-pass-2-selection.json) | [run-2](results/run-2/rust-pass-2-selection.json) |
| `rust-pass-2.json` | [run-1](results/run-1/rust-pass-2.json) | [run-2](results/run-2/rust-pass-2.json) |
| `typescript-pass-1-selection.json` | [run-1](results/run-1/typescript-pass-1-selection.json) | [run-2](results/run-2/typescript-pass-1-selection.json) |
| `typescript-pass-1.json` | [run-1](results/run-1/typescript-pass-1.json) | [run-2](results/run-2/typescript-pass-1.json) |
| `typescript-pass-2-selection.json` | [run-1](results/run-1/typescript-pass-2-selection.json) | [run-2](results/run-2/typescript-pass-2-selection.json) |
| `typescript-pass-2.json` | [run-1](results/run-1/typescript-pass-2.json) | [run-2](results/run-2/typescript-pass-2.json) |

### Cold samples

| File | Run 1 | Run 2 |
| --- | --- | --- |
| `rust-cold-1-html-selection.json` | [run-1](results/run-1/rust-cold-1-html-selection.json) | [run-2](results/run-2/rust-cold-1-html-selection.json) |
| `rust-cold-1-html.json` | [run-1](results/run-1/rust-cold-1-html.json) | [run-2](results/run-2/rust-cold-1-html.json) |
| `rust-cold-1-markdown-selection.json` | [run-1](results/run-1/rust-cold-1-markdown-selection.json) | [run-2](results/run-2/rust-cold-1-markdown-selection.json) |
| `rust-cold-1-markdown.json` | [run-1](results/run-1/rust-cold-1-markdown.json) | [run-2](results/run-2/rust-cold-1-markdown.json) |
| `rust-cold-2-html-selection.json` | [run-1](results/run-1/rust-cold-2-html-selection.json) | [run-2](results/run-2/rust-cold-2-html-selection.json) |
| `rust-cold-2-html.json` | [run-1](results/run-1/rust-cold-2-html.json) | [run-2](results/run-2/rust-cold-2-html.json) |
| `rust-cold-2-markdown-selection.json` | [run-1](results/run-1/rust-cold-2-markdown-selection.json) | [run-2](results/run-2/rust-cold-2-markdown-selection.json) |
| `rust-cold-2-markdown.json` | [run-1](results/run-1/rust-cold-2-markdown.json) | [run-2](results/run-2/rust-cold-2-markdown.json) |
| `rust-cold-3-html-selection.json` | [run-1](results/run-1/rust-cold-3-html-selection.json) | [run-2](results/run-2/rust-cold-3-html-selection.json) |
| `rust-cold-3-html.json` | [run-1](results/run-1/rust-cold-3-html.json) | [run-2](results/run-2/rust-cold-3-html.json) |
| `rust-cold-3-markdown-selection.json` | [run-1](results/run-1/rust-cold-3-markdown-selection.json) | [run-2](results/run-2/rust-cold-3-markdown-selection.json) |
| `rust-cold-3-markdown.json` | [run-1](results/run-1/rust-cold-3-markdown.json) | [run-2](results/run-2/rust-cold-3-markdown.json) |
| `rust-cold-4-html-selection.json` | [run-1](results/run-1/rust-cold-4-html-selection.json) | [run-2](results/run-2/rust-cold-4-html-selection.json) |
| `rust-cold-4-html.json` | [run-1](results/run-1/rust-cold-4-html.json) | [run-2](results/run-2/rust-cold-4-html.json) |
| `rust-cold-4-markdown-selection.json` | [run-1](results/run-1/rust-cold-4-markdown-selection.json) | [run-2](results/run-2/rust-cold-4-markdown-selection.json) |
| `rust-cold-4-markdown.json` | [run-1](results/run-1/rust-cold-4-markdown.json) | [run-2](results/run-2/rust-cold-4-markdown.json) |
| `rust-cold-5-html-selection.json` | [run-1](results/run-1/rust-cold-5-html-selection.json) | [run-2](results/run-2/rust-cold-5-html-selection.json) |
| `rust-cold-5-html.json` | [run-1](results/run-1/rust-cold-5-html.json) | [run-2](results/run-2/rust-cold-5-html.json) |
| `rust-cold-5-markdown-selection.json` | [run-1](results/run-1/rust-cold-5-markdown-selection.json) | [run-2](results/run-2/rust-cold-5-markdown-selection.json) |
| `rust-cold-5-markdown.json` | [run-1](results/run-1/rust-cold-5-markdown.json) | [run-2](results/run-2/rust-cold-5-markdown.json) |
| `rust-cold-6-html-selection.json` | [run-1](results/run-1/rust-cold-6-html-selection.json) | [run-2](results/run-2/rust-cold-6-html-selection.json) |
| `rust-cold-6-html.json` | [run-1](results/run-1/rust-cold-6-html.json) | [run-2](results/run-2/rust-cold-6-html.json) |
| `rust-cold-6-markdown-selection.json` | [run-1](results/run-1/rust-cold-6-markdown-selection.json) | [run-2](results/run-2/rust-cold-6-markdown-selection.json) |
| `rust-cold-6-markdown.json` | [run-1](results/run-1/rust-cold-6-markdown.json) | [run-2](results/run-2/rust-cold-6-markdown.json) |
| `rust-cold-7-html-selection.json` | [run-1](results/run-1/rust-cold-7-html-selection.json) | [run-2](results/run-2/rust-cold-7-html-selection.json) |
| `rust-cold-7-html.json` | [run-1](results/run-1/rust-cold-7-html.json) | [run-2](results/run-2/rust-cold-7-html.json) |
| `rust-cold-7-markdown-selection.json` | [run-1](results/run-1/rust-cold-7-markdown-selection.json) | [run-2](results/run-2/rust-cold-7-markdown-selection.json) |
| `rust-cold-7-markdown.json` | [run-1](results/run-1/rust-cold-7-markdown.json) | [run-2](results/run-2/rust-cold-7-markdown.json) |
| `typescript-cold-1-html-selection.json` | [run-1](results/run-1/typescript-cold-1-html-selection.json) | [run-2](results/run-2/typescript-cold-1-html-selection.json) |
| `typescript-cold-1-html.json` | [run-1](results/run-1/typescript-cold-1-html.json) | [run-2](results/run-2/typescript-cold-1-html.json) |
| `typescript-cold-1-markdown-selection.json` | [run-1](results/run-1/typescript-cold-1-markdown-selection.json) | [run-2](results/run-2/typescript-cold-1-markdown-selection.json) |
| `typescript-cold-1-markdown.json` | [run-1](results/run-1/typescript-cold-1-markdown.json) | [run-2](results/run-2/typescript-cold-1-markdown.json) |
| `typescript-cold-2-html-selection.json` | [run-1](results/run-1/typescript-cold-2-html-selection.json) | [run-2](results/run-2/typescript-cold-2-html-selection.json) |
| `typescript-cold-2-html.json` | [run-1](results/run-1/typescript-cold-2-html.json) | [run-2](results/run-2/typescript-cold-2-html.json) |
| `typescript-cold-2-markdown-selection.json` | [run-1](results/run-1/typescript-cold-2-markdown-selection.json) | [run-2](results/run-2/typescript-cold-2-markdown-selection.json) |
| `typescript-cold-2-markdown.json` | [run-1](results/run-1/typescript-cold-2-markdown.json) | [run-2](results/run-2/typescript-cold-2-markdown.json) |
| `typescript-cold-3-html-selection.json` | [run-1](results/run-1/typescript-cold-3-html-selection.json) | [run-2](results/run-2/typescript-cold-3-html-selection.json) |
| `typescript-cold-3-html.json` | [run-1](results/run-1/typescript-cold-3-html.json) | [run-2](results/run-2/typescript-cold-3-html.json) |
| `typescript-cold-3-markdown-selection.json` | [run-1](results/run-1/typescript-cold-3-markdown-selection.json) | [run-2](results/run-2/typescript-cold-3-markdown-selection.json) |
| `typescript-cold-3-markdown.json` | [run-1](results/run-1/typescript-cold-3-markdown.json) | [run-2](results/run-2/typescript-cold-3-markdown.json) |
| `typescript-cold-4-html-selection.json` | [run-1](results/run-1/typescript-cold-4-html-selection.json) | [run-2](results/run-2/typescript-cold-4-html-selection.json) |
| `typescript-cold-4-html.json` | [run-1](results/run-1/typescript-cold-4-html.json) | [run-2](results/run-2/typescript-cold-4-html.json) |
| `typescript-cold-4-markdown-selection.json` | [run-1](results/run-1/typescript-cold-4-markdown-selection.json) | [run-2](results/run-2/typescript-cold-4-markdown-selection.json) |
| `typescript-cold-4-markdown.json` | [run-1](results/run-1/typescript-cold-4-markdown.json) | [run-2](results/run-2/typescript-cold-4-markdown.json) |
| `typescript-cold-5-html-selection.json` | [run-1](results/run-1/typescript-cold-5-html-selection.json) | [run-2](results/run-2/typescript-cold-5-html-selection.json) |
| `typescript-cold-5-html.json` | [run-1](results/run-1/typescript-cold-5-html.json) | [run-2](results/run-2/typescript-cold-5-html.json) |
| `typescript-cold-5-markdown-selection.json` | [run-1](results/run-1/typescript-cold-5-markdown-selection.json) | [run-2](results/run-2/typescript-cold-5-markdown-selection.json) |
| `typescript-cold-5-markdown.json` | [run-1](results/run-1/typescript-cold-5-markdown.json) | [run-2](results/run-2/typescript-cold-5-markdown.json) |
| `typescript-cold-6-html-selection.json` | [run-1](results/run-1/typescript-cold-6-html-selection.json) | [run-2](results/run-2/typescript-cold-6-html-selection.json) |
| `typescript-cold-6-html.json` | [run-1](results/run-1/typescript-cold-6-html.json) | [run-2](results/run-2/typescript-cold-6-html.json) |
| `typescript-cold-6-markdown-selection.json` | [run-1](results/run-1/typescript-cold-6-markdown-selection.json) | [run-2](results/run-2/typescript-cold-6-markdown-selection.json) |
| `typescript-cold-6-markdown.json` | [run-1](results/run-1/typescript-cold-6-markdown.json) | [run-2](results/run-2/typescript-cold-6-markdown.json) |
| `typescript-cold-7-html-selection.json` | [run-1](results/run-1/typescript-cold-7-html-selection.json) | [run-2](results/run-2/typescript-cold-7-html-selection.json) |
| `typescript-cold-7-html.json` | [run-1](results/run-1/typescript-cold-7-html.json) | [run-2](results/run-2/typescript-cold-7-html.json) |
| `typescript-cold-7-markdown-selection.json` | [run-1](results/run-1/typescript-cold-7-markdown-selection.json) | [run-2](results/run-2/typescript-cold-7-markdown-selection.json) |
| `typescript-cold-7-markdown.json` | [run-1](results/run-1/typescript-cold-7-markdown.json) | [run-2](results/run-2/typescript-cold-7-markdown.json) |

## Supplementary datasets

These datasets use different timing boundaries from the report. Keep their
samples separate from the two paired runs. The
[location manifest](supplementary/manifest.json) records original paths, file
hashes and the source commit. Existing paths and hashes inside these files
remain unchanged.

| File | Contents |
| --- | --- |
| [reference/rust.json](supplementary/reference/rust.json) | Rust samples |
| [reference/typescript.json](supplementary/reference/typescript.json) | TypeScript samples |
| [reference/selection.json](supplementary/reference/selection.json) | Input selection |
| [reference/summary.json](supplementary/reference/summary.json) | Reference comparison and source identity |
| [native/baseline-1.json](supplementary/native/baseline-1.json) | Native baseline samples, first measurement |
| [native/baseline-2.json](supplementary/native/baseline-2.json) | Native baseline samples, second measurement |
| [native/candidate-1.json](supplementary/native/candidate-1.json) | Native candidate samples, first measurement |
| [native/candidate-2.json](supplementary/native/candidate-2.json) | Native candidate samples, second measurement |
| [native/verification.json](supplementary/native/verification.json) | Native source, output and measurement identity |
| [native/source.patch](supplementary/native/source.patch) | Measured native source delta |
| [native/tests.log](supplementary/native/tests.log) | Measured native test output |
