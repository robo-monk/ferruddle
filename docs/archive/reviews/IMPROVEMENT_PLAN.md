# Opus review: decision and improvement plan

Three independent Claude Opus 5.5 xhigh reviews are complete: architecture,
performance, and quality. The coordinating agent checked the important findings
against the source and reproduced the defects. The port needs a hardening pass
before release. Runtime code remains unchanged by this review.

## Fix correctness first

| Order | Finding | Required change | Proof of completion |
| --- | --- | --- | --- |
| 1 | Markdown can take exponential time on small nested HTML (D1/Q3) | Render each child once while preserving adjacent-inline merging. Precompute block containment rather than rescanning subtrees. | Deep nested-inline and unclosed-font regressions finish with bounded render work. Existing inline-run tests and all corpus output remain unchanged. |
| 2 | Unicode date text panics (D2) | Use fallible date parsing and explicit ASCII syntax for the supported English date format. | Both `٣ March 2024` and `3 Auguſt 2024` return normally. Valid dates still parse. Audit other input-derived unwraps. |
| 3 | Schema fallback can empty an article (Q1) | Exclude unsafe elements and their descendants from schema root matching. Fall back to sanitized schema text when no eligible DOM node qualifies. | Body JSON-LD and noscript teaser fixtures return the full 196-word article, matching the reference. Explicit unsafe user selection still returns empty output. |
| 4 | Dropbox links can remove paragraphs (Q2) | Restore social-profile boundaries and handle rules. | Positive profile URLs still match; Dropbox, Netflix, share/intent URLs, and unrelated hosts do not remove content. |
| 5 | Unsupported math cleanup removes source and a heading (Q4) | Share a math-source predicate across cleanup and rendering. Preserve safe source holders and surrounding prose. | The KaTeX data-math case keeps its heading and five sources. The exact warning remains. Re-pin only corrected behavior after reviewing the visible diff. |
| 6 | Relaxed cleanup can retain navigation/resource directives (Q7) | Remove meta/link directives in the unconditional sanitizer. Clean form controls in conversation adapters. | Poisoned adapter tests and tests with optional cleanup disabled reject these elements. Existing article and task-list content survives. |

Add minimal native fixtures that fail before each fix. Keep upstream TypeScript
expectations unchanged. Math conversion stays excluded; detection must still
return `math_not_supported` / `Math support is not yet included.` Preserving
source text is not a request to implement formula conversion.

Also fix false math warnings on currency/template/shell/path prose (Q6), and
preserve a separator for `br` inside inline code (Q8). These are small follow-ups
with direct input/output tests.

## Strengthen the quality gate

The existing fingerprints catch output changes, but an accepted fingerprint can
freeze a defect. That happened to the heading lost in the data-math case.

1. Store readable normalized output or semantic JSON alongside fingerprints.
2. Give each math case specific assertions for source and surrounding prose.
3. Add a reference-prose preservation check with block/cell boundaries and token
   multiplicity. Explicitly exclude formula syntax; require a reason for any
   intended prose loss. Keep the existing structural comparator.
4. Make the sanitizer gate cover the runtime URL attributes, srcset candidates,
   and forbidden elements. Pin adapter-materialized math warnings before moving
   detection out of retry attempts.

The new gate must expose Q4 before its expectation changes. Do not regenerate
all snapshots to accommodate the fixes.

## Apply the measured performance changes

Seven changes were tested together in a scratch copy. The four final runs use
the project's harness with all 208 non-math cases, two warmups, and seven rounds.

| Mode | Baseline batch medians | Experimental batch medians | Time reduction |
| --- | --- | --- | --- |
| HTML | 851 / 843 ms | 431 / 438 ms | 48–49% |
| HTML + Markdown | 866 / 857 ms | 449 / 454 ms | 47–48% |

The root agent verified byte-identical output for all 226 cases, including debug
output. The reviewer's release log records 170 passing tests. This proves the
experiment preserves current behavior, including the known defects above.

Apply the changes in this order, with checks after each group:

1. **Bound byline ancestor work (P1).** Stop at the first ancestor over 15 words;
   stop counting once the threshold is exceeded. This change plus local predicate
   reorderings reduced corpus time by about 30%. The LessWrong case is the main
   hotspot. Test counter equivalence on split words, CJK, and whitespace; share
   the character classification rather than copying it.
2. **Use intended ASCII word boundaries (P2).** Two regex changes gave a further
   8.5% reduction in that experiment sequence. They align the boundary rule with
   JavaScript. Pin behavior at non-ASCII boundaries before changing it.
3. **Simplify the newsletter regex (P4).** Replacing `[\s\S]` with `(?s:.)` cut
   measured first-call cost from about 38 ms to 19 ms. Test newline coverage.
4. **Run cheap predicates first (P7).** Check hidden status before math-descendant
   scans, and partial-selector matches before protected-subtree scans. Measured
   gain was about 5% in its experiment sequence.
5. **Reuse main selection across retries (P3).** This gave a further 5.6%
   reduction. Preserve clone-stable node IDs and separate adapter selectors.
6. **Add short-circuit query helpers (P6).** An `exists` helper avoids collecting
   every match for a yes/no question. Its measured gain was about 1.5%, near the
   noise floor; it also clarifies intent.
7. **Consider a fixed-list selector index (P5).** It gave a further 10.4%
   reduction but adds about 110 lines. Keep the real matcher as the final test.
   Fix conservative fallback and test equivalence before adoption. The prototype
   scanner mishandles non-ASCII identifiers and tab combinators; do not expose it
   as a general CSS parser.

These percentages come from different stages of a cumulative experiment. Do not
add them. The full combined result is the table above. Some individual cases
were 5–9% slower in a run, so the reviewer's blanket no-regression claim does not
hold. Repeat those cases before drawing a conclusion about noise.

No TypeScript run accompanied these experiments. Re-run both implementations
after the final changes before updating the existing TS/Rust speed ratios.
Keep LessWrong in the headline result. Report aggregate batch time, per-case
median/p95, cold-call cost, and the slower cases.

## Improve structure without a broad rewrite

- Split pipeline work into **prepare, attempt, finish**. Compute shared selection,
  math detection, and base URL once. Keep candidate DOMs until the winner is known;
  render Markdown once. Selection reuse alone was measured; the broader design
  has no separate speed estimate yet.
- Keep sanitized word counts and the hidden-retry HTML-length comparison exactly
  where winner selection requires them. Preserve debug records and the final
  sanitizer after transforms and URL resolution.
- Replace the adapter's two optional content fields with an enum. Make document
  mutation explicit and pass parsed URLs through adapters.
- Make hidden public implementation modules private. Move internal tests into
  crate-local modules or use a narrow test facade. Keep only the intended API
  public; document metadata as untrusted page strings.
- Consolidate DOM tag/retag/matcher helpers and math-source rules. Add extensible
  warning/error enums. Keep `Options { ..Default::default() }` usable.
- Clarify standalone Markdown behavior: leading H1 removal, relative links, and
  declarative shadow content. Intraword spacing changes are optional quality
  work because the current behavior matches TypeScript.

The reviewers found no reason to replace the dependencies, rewrite the DOM,
expand the selector cache, or split large cohesive modules only by line count.
Protected-subtree traversal, title-regex reuse, and wider selector indexing are
later experiments. Profile again after the measured changes.

## Final acceptance

Run the new regressions, all 226 corpus cases, the full native suite, formatting,
Clippy with warnings denied, public API examples, and CLI smoke checks. Verify
that upstream hashes remain unchanged. Review every intended output change.
Then run the full TS/Rust benchmark under comparable conditions, with alternating
mode/run order and no competing heavy work. Keep source and harness hashes.

Two reviewers encountered crashes in the default Rust toolchain. Performance
tests used a separate rustc 1.99.1 installation. A working reproducible toolchain
is required for the final validation; the prototype has not passed fmt/Clippy.

Evidence: [architecture](opus-architecture.md), [quality](opus-quality.md),
[performance](opus-performance.md), [root verification](root-verification.md),
[raw experiment and verification](performance-evidence/verification.json), and
[unapplied prototype patch](performance-evidence/experiment.patch).
