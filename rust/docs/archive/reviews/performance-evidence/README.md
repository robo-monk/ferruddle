# Preserved performance experiment

These are review artifacts. No experiment has been applied to `rust/src`.

- `experiment.patch`: normalized E1–E7 scratch diff, relative to the measured
  source. `git apply --check rust/reviews/performance-evidence/experiment.patch`
  succeeds at the repository root. The selector-index prototype needs the
  corrections and tests described in the improvement plan before adoption.
- `bench7-base-{1,2}.json`: the two final baseline runs.
- `bench7-exp6-{1,2}.json`: the two final combined-experiment runs, retaining the
  reviewer's original filenames. Each has both modes, 208 cases, seven timed
  rounds, and two warmups. No TypeScript comparison was run in this experiment.
- `experiment-tests.log`: the reviewer's saved release-test summary, 170 passes,
  seven ignored diagnostic helpers, exit zero. Formatting/Clippy were not run.
- `verification.json`: hashes of these artifacts, recomputed benchmark metrics,
  and the per-file hashes of the 226 byte-identical output dumps checked by the
  coordinating agent. The full scratch dumps remain temporary.

The experiment preserves existing defects. Its output equivalence is evidence
for optimization safety on the pinned corpus, not a replacement for the
correctness fixes and new regression cases.

The complete method and cold-call experiments are described in
[the performance review](../opus-performance.md). Scratch paths in that review
can expire. Apply the reviewed changes to a separate copy, then use the existing
`rust/examples/benchmark.rs` and pinned selection to reproduce native A/B runs.
For the final comparison, run both sides with:

```bash
node scripts/porting/benchmark.mjs rust/benchmarks/after-hardening 7 2
```

Keep the full selection, including LessWrong. Repeat the cases with apparent
slowdowns before claiming no per-case regression.
