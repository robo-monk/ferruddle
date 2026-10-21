# Review verification by the coordinating agent

This records checks of the independent Opus 5.5 xhigh reviews. Runtime source,
tests, corpus expectations, contracts, and benchmark results remain unchanged.

## Architecture findings

- **Confirmed: page-controlled Unicode dates panic.** The existing debug CLI
  exits 101 for `٣ March 2024` and `3 Auguſt 2024` in a paragraph after an article
  heading. An ASCII control, `3 March 2024`, succeeds and returns
  `2024-03-03T00:00:00+00:00`. The two panics occur at `metadata.rs:701` and
  `metadata.rs:698`, respectively. Unicode-aware digit matching and case folding
  conflict with the ASCII parsing assumptions. Use fallible parsing and explicit
  ASCII date syntax, with regressions for both inputs.
- **Confirmed by code: repeated rendering has an exponential recurrence.**
  `Renderer::children` fully renders the next non-block sibling to test whether
  it is empty, then discards nonempty results. Nested occurrences repeat that
  work. The depth guard bounds recursion depth, not total work. Bounded local
  CLI probes at depths 6, 8, and 10 completed and reached the affected path.
  The architecture review supplies the larger scaling reproduction. Its debug
  timings are illustrative, not controlled benchmark results. Reuse work while
  preserving inline-run merging; add deep nested input regressions and require
  unchanged corpus output. Treat this as an availability defect, not merely an
  optimization opportunity.
- **Confirmed by code: losing retries render Markdown.** Each `attempt` calls
  `finish`, which renders Markdown whenever requested. Retry decisions use word
  count and, in one branch, serialized HTML length. Main-content selection and
  math scanning also repeat on clones of the same prepared document. Preparing
  shared state once and rendering only the winning candidate is a strong
  structural proposal. Its speed benefit remains unmeasured at this point.
- **Confirmed by code: avoidable full-subtree work.** Hidden-element removal
  checks for math descendants before testing whether a node is hidden;
  `remove_empty` obtains recursive text before testing child kinds; whitespace
  boundary handling obtains full subtree text for first/last character checks.
  These are candidates for local reordering or bounded traversal, subject to
  corpus and debug-output verification.
- **Confirmed API exposure; severity qualified.** Seven `#[doc(hidden)] pub mod`
  declarations expose implementation types and helper entry points, including
  raw adapter output. The intended top-level extraction and conversion APIs
  still sanitize. This is an API and maintenance boundary issue; the review did
  not demonstrate a sanitization bypass in those top-level APIs. Prefer moving
  internal tests into crate-local modules or using a deliberate test facade;
  choose the smallest design that keeps normal downstream API surface private.

Nesting-depth limits, body-versus-selected-root removal scope, and a stricter
no-panic lint policy remain design proposals. They are not established fixes or
measured speedups. Any eventual limit must be explicit and must not silently
truncate article content. Static-regex construction and genuinely proven
invariants need different treatment from page-derived parsing.

## Quality findings

Independent root probes reused the review's minimal HTML inputs with the existing
native CLI and compared selected cases with the unchanged TypeScript build.
These are targeted reproductions, not a new full-suite verification.

- **Confirmed Q1: schema fallback returns empty content.** Both body JSON-LD and
  a body `noscript` containing the schema article text produce zero native words
  and empty content. TypeScript returns the 196-word schema article in both
  cases. `schema_element` searches unsanitized candidates and can force an unsafe
  root, after which sanitization correctly empties it. Search eligible visible
  content only, and retain the sanitized schema-text fallback when no safe node
  qualifies. Keep explicit user selection of unsafe roots a separate invariant.
- **Confirmed Q2: social-profile detection loses ordinary content.** The control
  page linking to `example.org` retains its download paragraph and has 124 words;
  replacing only the host with `www.dropbox.com` removes it and gives 113 words.
  TypeScript keeps the Dropbox paragraph and has 124 words. Substring matching of
  `x.com/` lacks a host/word boundary. Restore the intended social-profile test,
  with both positive profile links and negative unrelated-host/share links.
- **Confirmed Q4: unsupported math cleanup loses surrounding prose.** A minimal
  `data-math` input loses its `Quadratic Formula` heading and all source-bearing
  attributes. Its ending becomes `For all, it holds.` The no-conversion policy
  does not justify deleting article headings. Preserve safe source and ordinary
  prose while retaining the existing warning; do not add math conversion.
- **Confirmed Q5: the math contracts need stronger assertions.** All 18 math
  cases share one reason string. Their hashes freeze output but do not prove
  source or prose preservation. The identified loss is therefore a genuine gap
  in the earlier quality assessment. Add readable semantic snapshots and
  case-specific preservation claims. A prose-preservation assertion should
  handle block/cell boundaries, token multiplicities, and explicitly excluded
  formula syntax; do not replace semantic checks with a word-set comparison.
- **Confirmed Q6: currency prose can warn incorrectly.** A paragraph containing
  `$10 per user_id and $20 per team` raises `math_not_supported`. Tight dollar
  delimiters and additional negative technical-prose tests should reduce false
  positives without changing the 18 true-positive corpus cases.
- **Confirmed Q7 adapter cleanup gap; broader consequence is code-inspected.**
  The ChatGPT conversation probe retains meta/link/form/input/button elements;
  TypeScript removes them. Under defaults, attributes on meta/link are mostly
  stripped, so this probe does not itself demonstrate active refresh or CSS
  loading. With `standardize: false`, the code path skips that stripping and the
  always-on sanitizer does not reject meta/link. Remove navigation/resource
  directives unconditionally and pin this with all-cleanup-disabled tests. Any
  claim about browser execution remains untested in this review.
- **Confirmed Q8: inline code joins words across `br`.** Native Markdown renders
  ``Run `makeinstall` now.`` whereas TypeScript preserves separation.
  Map authored breaks to whitespace before code-span normalization.

Q3 independently reaches the same exponential renderer as architecture D1 with
unclosed legacy `font` markup. It is one defect, not a second optimization.
Q9's shared math-source predicate and Q10/Q11's API/selection contracts are
useful follow-ups. Q12 (intraword spacing) matches TypeScript; changing it would
be a deliberate quality improvement, not a required parity fix. Q13 concerns
readability of raw Markdown only.

Guardrails for the proposed pipeline optimization: sanitize before word-count
decisions, preserve the hidden-retry serialized-length comparison, detect math
materialized by adapters, retain the final post-transformation sanitizer, and
preserve outer-versus-inner serialization and retry winner selection. Keep all
208 non-math benchmark cases, including LessWrong; its outlier cost is a reason
to profile, not a reason to omit it from the headline comparison.

## Performance findings

- Recomputed hashes for all 29 native source/package entries and 76 reference
  and harness entries in the saved benchmark summary. All match. The review did
  not change the measured implementation or upstream files.
- Independently compared all 226 baseline and final-experiment output dumps.
  Every file is byte-identical, including HTML, Markdown, metadata, warnings,
  debug content, selector paths, and removal records. This preserves known
  defects too; it is equivalence evidence, not evidence that those defects were
  fixed.
- Parsed the saved release test log: 170 passes, seven ignored diagnostic
  helpers, exit zero. This is the reviewer's test run, not a fresh root run.
  Formatting and Clippy were not run on the experimental patch.
- Recomputed batch medians from the four final raw benchmark files. HTML time
  falls by 48.0–49.4%; HTML plus Markdown time falls by 47.0–48.1%. The files
  contain 208 cases, seven rounds, and two warmups. TypeScript was not rerun;
  these figures cannot establish a new TypeScript-to-Rust ratio.
- **Correction to the performance report:** the statement that no case became
  more than 5% slower is unsupported by the saved data. Individual run ratios
  reach 1.095 in HTML mode and 1.080 in Markdown mode. Pooled two-run ratios
  slightly exceed 1.05 for Hacker News HTML and GitHub/X article Markdown.
  These small regressions may be noise, especially on submillisecond cases,
  but require targeted alternating reruns before a no-regression claim.
- The corpus profile puts ordinary Markdown rendering at 2–3% of runtime. This
  does not reduce the priority of D1: a normal corpus cannot rule out an
  exponential path on another valid input.

The best demonstrated performance change is bounding the byline ancestor walk
and stopping text counting once the small-word threshold is exceeded. The
ancestor count is monotone because each ancestor's concatenated text contains
the descendant's text. The helper must preserve word-count state across text
nodes and use the same CJK/whitespace classification as the current counter.

The selector index is a useful measured prototype, not a finished general CSS
optimization. Its key scanner accepts partial ASCII identifiers: `.café` becomes
the necessary key `caf`, which can exclude a correct match. It also recognizes
only a literal space as descendant-combinator whitespace, so `div\t.x` can be
bucketed by the wrong compound. The current fixed list passes the corpus, but
broader use needs a parsed-selector API or conservative fallback, plus Unicode,
escape, whitespace, namespace, and nested-pseudo tests.

Preserved the experimental patch, final raw benchmark files, test log, and
output hash manifest under `performance-evidence/`. The normalized patch passes
`git apply --check` against the current tree; it was not applied. The report's
temporary scratch paths should not be treated as durable reproduction assets.
