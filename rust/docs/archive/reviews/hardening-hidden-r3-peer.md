# N4 hidden-retry peer verification

Reviewer: GPT-6.1 Sol, xhigh, independent read-only peer. This is not an Opus
review. Scope is the N4 candidate-selection change only.

**No defect found.** The new search matches TypeScript: body descendants only,
case-sensitive exclusion of classes containing `math`, first strict maximum in
document order, and a minimum of 30 words. Each eligible candidate is counted
once. A zero-word maximum produces no candidate, as in TypeScript.

## Preprocessing and selected-root positions

TypeScript's `findLargestHiddenContentSelector` clones the document, flattens
shadow content, and resolves streaming swaps before constructing its selector.
The retry performs the same preprocessing before resolving that selector.
Rust scans `prepared.original`, which was already preprocessed, and forces the
selected clone-stable node ID into the attempt. It does not reuse a selector
built against pre-hoisting or pre-streaming sibling positions.

Six anonymous public-API probes independently matched TypeScript/linkedom debug
**HTML byte-for-byte and word counts exactly**:

| Probe | Before N4 | After N4 and TypeScript |
| --- | --- | --- |
| Hidden body | body, 39 words | main, 3 words |
| Equal nested candidates | inner div | outer section, 36 words |
| Shadow hoist shifts siblings | inner div | section at body child 4 |
| Shadow hoist changes equal-candidate order | later inner div | first shadow section |
| Streaming swap shifts siblings | inner div | streamed section at slot child 2 |
| Streaming swap changes equal-candidate order | later inner div | first streamed section |

The last four probes establish both selection identity and the resulting
post-preprocessing root location. All focused candidates contain the same
36-word text. The two repository fixtures and their exact TypeScript debug
expectations agree with these results. Root's retained logs show the two public
regressions failing before and passing after; this peer did not use the shared
current target to rerun them.

## Isolated complete-output proof

A dedicated source copy held Markdown fixed on both sides. Only `pipeline.rs`
was changed between runs:

| Input | SHA-256 |
| --- | --- |
| Before pipeline | `c4a24058422ba947d7a724579abd245ad9c0158169ca65e5c62a149265ee71f8` |
| After pipeline | `c3ac5aeb13c7f57ee5347195263339f7378465008cad187a971673aa632fe49c` |
| Fixed Markdown | `967da51e33499e4065d6081886cf98d60825caee64d335bdb4154514e7dec61d` |

**All 452 complete extraction results are byte-identical:** all 226 upstream
cases, each with Markdown enabled in default and debug modes. This includes
metadata, HTML, Markdown, warnings, word counts, selectors, and ordered debug
removals. Both serialized result files have SHA-256:

`5846a678573c65219e0b1ed00436bcf070eddba4fc24c6fa73339a0ef55c03ef`

All 226 upstream file and input-HTML hashes were checked against their manifest
and case declarations. This is a diagnostic equivalence check, not a benchmark
or full-suite run.

## Read set and evidence

Read: `hardening-opus-performance-r2.md`; Rust `pipeline.rs`, `dom.rs`,
`utils.rs`, and hidden-selector constants; TypeScript `defuddle.ts` retry,
selection, selector construction, shadow and stream methods; its constants and
word counter; `hardening_pipeline.rs`; the two hidden fixtures and expectations.
The corresponding compiled TypeScript methods and linkedom compatibility helper
were inspected/executed for the reference probes.

Evidence directory:
`/home/robomonk/.cache/defuddle-port-hardening/hidden-r3-peer/`.
It contains `read-set-hashes.json` (19 unchanged reviewed/probe-input files),
`current-source-hashes.json`, `before-source-hashes.json`,
`after-source-hashes.json`, `input-hashes.json`, `equivalence.json`, both complete
corpus JSON files, probe inputs, and before/after/TypeScript probe outputs.
Only the pipeline hash differs between the two 30-file source/Cargo maps.

The copy was built with the working 1.99.1 environment and dedicated
`target-hidden-r3-peer`. No runtime, test, contract, snapshot, or harness was
edited. Release-wide validation and benchmarking remain with the root agent.
