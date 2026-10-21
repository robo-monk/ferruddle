# Response to performance review: benchmark evidence

This response addresses section 2 of `hardening-opus-performance-r1.md`.
No new headline timings have been collected yet. The historical 2026-10-20
results remain unchanged and are not used to infer the hardening gain.

- Samples below 20 report a median, upper quartile, minimum, maximum, and
  count. They omit sample p95; the p95 over 208 page medians remains labeled.
  A seventh pure harness test checks the small-sample behavior.
- `benchmark-native-repeat.mjs` automates baseline/current/current/baseline
  repeats with the exact shared worker, both original mode orders, complete
  raw-run validation, self-stable outputs, and per-pass/pooled per-case results.
  It lists changed output proofs rather than demanding old/new equality.
- Native repeats report current pass-to-pass A/A fold differences and flag
  slower cases only descriptively. The noise-aware flag requires both passes
  to exceed the larger of 5% and the p95 A/A fold difference across 208 pages.
  This is not a significance test or proof that all pages improve.
- Both drivers run copied executables from the result directory and check
  their hashes around every worker. The repeat also verifies the original
  runtime against historical source hashes and checks worker/input contracts.
- Provenance now includes `tsconfig.json`, the installed dependency lock,
  existing Cargo config/toolchain files, `rustc -vV`, the CPU governor when
  available, and hashes of Cargo/Rust environment values. Secret-looking
  variable names are excluded; arbitrary values are not printed.
- Results explicitly label the reference as TypeScript source under
  Vitest/Vite SSR with linkedom. They document V8 collection opportunities
  during untimed validation and do not force GC.
- The release worker will be pre-built. A complete discarded three-round
  smoke run will exercise extraction/oracle/determinism/cold paths before the
  quiet final window. Its timings will not be published as benchmark results.

The measurement method and script README document the automated repeat.
Seven pure checks and the real 208-input validation pass. A discarded complete
three-round, one-warmup smoke run also passed: both 208-case implementation
passes, exact reference projection checks, all fresh-process cold calls,
cross-pass/cold output stability, and the four-run native repeat. The native
repeat validated original/current source and copied executable identities and
produced complete A/A/per-page statistics. Its evidence is local under
`~/.cache/defuddle-port-hardening/benchmark-smoke-r2`; these timings are not
published as benchmark results. Both smoke command logs exit zero.

A second independent review, final quiet measurements, and README replacement
remain pending. The release build is already complete so the final harness
build will be a no-op.
