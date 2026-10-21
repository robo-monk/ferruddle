# Coordinating-agent hardening checks

The reference-prose gate fails on the preserved baseline only for
`math--katex-data-math`: the words `Quadratic` and `Formula` are missing from
both HTML and Markdown. It passes all 226 cases after hardening.

All 18 math contracts compare original source signatures and multiplicity in
HTML and CommonMark-rendered Markdown. The source counts total 128 plus raw TeX.
The first complete native run passed 210 tests with 8 ignored diagnostics; the
unchanged TypeScript suite passed all 826 tests. These are intermediate results;
final verification and benchmark evidence are recorded separately.

## Additional untrusted-input panic

A site-name metadata value of 300,000 ASCII `A` characters causes the baseline
CLI to panic in `metadata::clean_title`: compiling its page-derived literal regex
exceeds the 10 MiB regex budget, and `unwrap` aborts extraction. The exact input
in `oversized_site_metadata_cannot_panic_during_title_cleanup` reproduces this
with the same page URL. The baseline exits 101 with `CompiledTooBig(10485760)`.

The fix treats site-pattern compilation as fallible and continues the existing
best-effort title cleanup. It adds no heuristic or content-size limit. The new
public API regression checks that article title and body still return. This
addresses the no-panic input audit; it does not claim to remove the remaining
per-document title-regex compilation cost.

## Validation environment

The prior stable compiler crashed before entering compiler code. Coredump
records show SIGSEGV in unsymbolized `ld-linux` frames, including `rustc --version`;
this was not an out-of-memory kill. The exact cause of that installation damage
was not established by the coordinating agent. The original stable installation
and default selection were left unchanged. Fresh named Rust 1.99.1 and 1.88.0
installations are used for validation; the latter checks the declared minimum.

Baseline builds now have a separate Cargo target directory. Initial worker
baseline runs reused the current target and briefly linked stale artifacts;
bounded probes detected this, workers rebuilt the current source, and the full
native test run above used the rebuilt current crate. Definitive final results
must identify source hashes and a current-only build directory.

## Round-two integration

The coordinating agent independently reproduced the R1 quote-attribute,
superscript, and table-blank-line Markdown injection probes, plus lost emphasis
after block math. Frozen before-output evidence is retained as
`~/.cache/defuddle-port-hardening/root-review-r1-before.json`.

After the fixes, the complete current suite passes **233 tests**, with eight
explicit diagnostics ignored. All 226 prose/security/semantic/readable-snapshot
contracts pass without any further snapshot or allowlist edit. The diagnostic
report still records 168 exact non-math Markdown outputs, 189 semantic matches,
19 reviewed differences, and 181 HTML semantic matches. All 18 original math
source contracts now run in default, debug, and standalone modes.

Strict Clippy, formatting, Rust 1.88 library/CLI checks, and a fresh release CLI
HTML/Markdown/JSON/warning/sanitization smoke pass. The 67 original TypeScript,
package, and fixture-harness hashes still match. The unchanged TypeScript suites
previously passed 826 tests each under linkedom and JSDOM.

The standardization and lazy-schema performance follow-ups each have isolated
452-result complete normal/debug before/after byte-equivalence evidence. The
schema trim-order correction additionally has a minimal fail-before/pass-after
fixture. Review responses identify the precise source snapshots and evidence.
The final independent review and performance measurements remain separate.

## Final source and measurements

The final frozen source passes **242 Rust tests**, with eight explicit
diagnostics ignored, strict Clippy, rustfmt, the Rust 1.88 library/CLI check,
and a fresh release CLI smoke. The source and executable hashes in
`hardening-run.json` and the benchmark evidence identify the exact build.
Durable [validation logs](validation/manifest.json) include the unchanged
826-test TypeScript runs for each DOM backend.

The provider interrupted the second Opus correctness review. Its partial
retained-HTML-block finding was fixed without switching the requested model.
Root replayed all 40 inert-marker cases: 28 activated a marker before the fix;
zero do afterward, with all literal marker text preserved. An independent
GPT-6.1-Sol xhigh peer checked 2,082 cases with pulldown and Marked/JSDOM plus
54 exact source assertions, finding no blocker. This does not claim a completed
second Opus correctness review; its [status](hardening-opus-correctness-r2-status.md)
and the [peer review](hardening-markdown-peer-r3.md) state that limit.

Root also adopted the hidden-retry candidate-scope/tie correction from the
second performance review. Both new anonymous fixtures fail before the fix and
pass afterward against the original TypeScript debug HTML. A separate peer
verified six ordinary/shadow/streaming probes and byte-identical complete
normal/debug outputs on all 226 cases. The [report](hardening-hidden-r3-peer.md)
records source identity and the 452-result proof.

All 226 native readable/structural snapshots remain unchanged after these last
fixes. Only three Markdown spellings changed: literal tag-like text now uses
entities, and safe autolinks use explicit Markdown. Exact non-math reference
Markdown becomes 165; semantic matches remain 189 plus 19 reviewed differences.
The [comparison](hardening-r3-quality.json) pins before/after hashes without
changing any existing quality contract. HTML and all other extraction fields
remain identical on every corpus case.

Two final source-stable benchmark runs and their native repeats are complete.
Every output proof, runtime/harness/input hash and executable hash agrees across
the runs. Reviewer-authored independent Python programs recomputed every final
statistic and flag with `ALL_MATCH`. The benchmark baseline's source is archived
with all 259 file hashes, allowing reproduction after the private cache is gone.

The observed ranges are 7.29–7.48× for HTML and 7.38–7.61× for HTML+Markdown
against the TypeScript source under Vitest/linkedom. The same-worker native
comparison lowers corpus time by 38–40%. Some individual slowdown flags vary
across repeats; no blanket absence-of-regression claim is made.

The host had existing GPU-driver waits and active paging. Five-second vmstat
records show 85–86% median CPU idle, with measurable swap IO in both windows.
The [benchmark report](../BENCHMARKS.md) records the exact limits, all per-page
results, raw monitor logs, source archives, and reproduction commands. The data
are descriptive host measurements, not an isolated-machine guarantee.
