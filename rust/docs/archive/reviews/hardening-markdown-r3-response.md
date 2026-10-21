# Renderer response to the interrupted review's raw-HTML finding

Implemented the partial second review's raw-HTML boundary fix in
`src/markdown.rs`, with regressions in `tests/markdown.rs` and two anonymous
fixtures: `retained-math-image-break.html` and
`retained-math-block-boundaries.html`. No snapshots, source contracts, allowlists,
or upstream files were changed. This is an implementation response to a partial
finding; it does not represent completed Opus review or sign-off.

## Failure before implementation

New regressions were run first against the existing current runtime:

- A retained image with `data-tex` before a hard break starts a CommonMark type 7
  HTML block. The following escaped page text creates an actual `u[data-probe]`
  node instead of literal `<u data-probe>marker</u>` text.
- The tag-coverage regression first fails on `basefont`, showing that the problem
  is broader than the initial menu/search/legend examples.
- The block-boundary fixture fails with actual marker elements in list items,
  blockquotes, and prose after legend/textarea holders.
- Text resembling `<!--@x-->` also starts an HTML block under the previous broad
  email exception, activating the marker after it. PI and CDATA examples exercise
  the same family of weak-autolink cases.

The coordinating agent independently reproduced actual marker elements in 28 of
40 reviewer inputs/modes before the fix. Its evidence is cached in
`~/.cache/defuddle-port-hardening/root-r2-before.json`. The original reviewer
inputs remain under `opus-review-r2/inputs-inert/` in that cache.

## General fix

All retained HTML now passes through one Markdown-boundary helper after the
structural serializer introduced in the previous response:

1. CommonMark's complete 62-tag type 6 list and the four type 1 raw-text starts
   receive blank-line separation. This includes tags such as menu, search,
   legend, nav, iframe, and textarea instead of relying on a subset of article
   block elements.
2. Other retained HTML void roots receive a static inline span wrapper. A lone
   image tag before a hard break can no longer become a type 7 block. The image
   keeps its source attributes and stays in the same paragraph as its hard break
   and following emphasis; it is not forced into display-block Markdown.
3. Literal non-autolink `<` uses `&lt;`, since backslash escapes are ignored in an
   HTML block. A crate-local rendered-DOM regression deliberately puts escaped
   page text inside an unexpected raw block and verifies no marker becomes HTML.
4. Actual safe HTTP/email autolinks emit explicit Markdown links. HTTP schemes
   and CommonMark's ASCII email local/domain grammar are recognized explicitly;
   merely containing `@` is no longer enough. No page-derived raw `<` is emitted
   even for valid email local parts containing slash or equals. Tests preserve
   link destinations, including literal entity spellings in URL text.

Native prose containment and CommonMark block containment are separate caches,
computed together in one bottom-up traversal. CommonMark's treatment of iframe,
textarea, or script does not turn an inline source holder into a prose section.
In particular, a genuine math holder containing img/br or an inert math script
retains its source attributes. The existing once-per-node render assertion and
depth-200 regressions continue to pass.

## Checks

The public tag regression exercises every type 6 tag and all four type 1 starts,
then checks literal marker text, following emphasis, a heading, and list items
through GFM-rendered HTML. Unsafe or invalidly positioned input tags may be
removed/repositioned by the HTML parser; following prose must still remain safe
and structured. The two minimal fixtures additionally run in standalone,
default extraction, debug extraction, and extraction without standardization.
Inline image source attributes and paragraph placement are checked directly.

Using `~/.cache/defuddle-port-hardening/env.sh`:

- Final Markdown integration suite: **53 passed, zero failed, two optional
  diagnostics ignored** (55 total), including the mixed img/br source regression
  in all four modes. The suite completed in 2.05 seconds.
- Three crate-local Markdown tests passed: unexpected-HTML-context text defense,
  caption-only render-once, and the depth-200 merge-lookahead guard.
- All eight `math_safety` tests passed, including all 18 source contracts compared
  through extracted HTML and GFM-reparsed Markdown in default, debug, and
  standalone modes.
- Owned runtime/tests were formatted with rustfmt. No benchmarks were run.

Frozen runtime SHA-256:
`967da51e33499e4065d6081886cf98d60825caee64d335bdb4154514e7dec61d`.
The coordinating agent owns the independent 40-case after matrix, full suite,
snapshot review, and final validation. A separate peer was notified of the source
freeze for read-only review of HTML types 1/6/7, autolinks, split text, and source
preservation.

## Separate observations and accepted limits

The previous response's nonempty iframe fallback limitation remains: character
references in iframe raw text do not decode when reparsed. The coordinating agent
explicitly chose to retain the safe serializer behavior, without inventing
visible fallback content or product attributes. This fix separates following
Markdown from iframe holders; it does not alter that decision.

A broader mixed-holder probe also exposed a separate image-normalization case:
a source span with an image and caption-like child can become a source figure
with figcaption before rendering. The prose-holder guard then renders its
children and drops the figure's source attribute from Markdown. This predates
the present boundary fix and was reported to the coordinating agent. The new
minimal img/br regression excludes caption synthesis so it isolates containment
and void-boundary behavior rather than changing the expectations for that issue.

Math conversion remains excluded. The existing unsupported warning message and
code are unchanged.
