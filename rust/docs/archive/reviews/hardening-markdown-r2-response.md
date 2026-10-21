# Markdown response to correctness review F1, F3, and F5

Fixed F1a/F1b, F3, and F5 in `src/markdown.rs`, with rendered-DOM regressions in
`tests/markdown.rs`, a retained-pre fixture in
`tests/data/markdown/retained-pre-breaks.html`, and a crate-local caption test.
No snapshots, allowlists, upstream files, or source contracts were changed.

## Reproduction before the fixes

The new tests were first run against the current runtime before implementing the
fixes, using the environment in
`~/.cache/defuddle-port-hardening/env.sh`. No runtime copy or baseline target was
needed.

| Finding | Failing assertion before implementation |
| --- | --- |
| F1a | A quote-bearing attribute name in a retained math holder produces an active `javascript:alert(1)` link after CommonMark rendering. |
| F1b | Separate tests for sup/sub, SVG, media, unusual lists, and retained tables all produce active `javascript:` links from literal page text. |
| F3 | Block math followed by ordinary Markdown prose produces only one of two expected emphasis nodes; the following prose contains literal backslash escapes. |
| F5 | A caption-only table immediately trips `Markdown must render each node at most once` in the crate-local regression. |

The initial integration run reported 39 passed, six failed, and two ignored. The
separate block-math regression failed after adjusting its setup to place the math
holder at the start of a Markdown line, matching the review's trigger. The
caption-only regression failed immediately. Root's independent frozen probe
evidence is in `~/.cache/defuddle-port-hardening/root-review-r1-before.json`.

## Implementation

One iterative structural serializer now handles every retained-HTML path: math,
sup/sub/SVG, media fallbacks, unusual ordered lists, and complex tables. It walks
the already-sanitized DOM rather than scanning serialized markup for quotes.

- Element names must match CommonMark's ASCII tag grammar. Invalid names are
  unwrapped while their children remain protected literal content.
- Attribute names must match `[A-Za-z_:][A-Za-z0-9_.:-]*`. Invalid attributes are
  omitted. Valid SVG case and namespace-qualified attributes are preserved.
- Attribute values are escaped at emission, including quotes, ampersands, angle
  brackets, newlines, carriage returns, and tabs.
- Text punctuation and LF/CR/tab use numeric HTML references. This keeps page
  text literal through Markdown parsing and prevents blank lines from ending
  retained HTML before its closing tags.
- Inert math scripts emit code source without script attributes. Exact script
  text, including literal entity spellings, newlines, TeX, tabs, and Unicode,
  survives HTML reparsing. Legacy raw-text holders use the same code treatment.
- An already-parsed leading LF in pre/textarea/listing receives an additional
  LF reference so HTML's initial-newline rule preserves the original text.

Raw native-block math output now uses `block()` to separate it from following
Markdown prose. Tests verify headings, list items, strong/emphasis nodes, literal
asterisks/brackets, and the absence of displayed Markdown escape backslashes
before and after the holder, in standalone/default/debug/nonstandardizing modes.

The narrowed math-source predicate supplied by the math worker is guarded by the
renderer’s existing native-block containment cache: holders with native prose
block descendants render their children normally. This supports F2 without new
descendant scans during rendering.

Table captions are evaluated lazily, only on the branch that consumes them.
Zero-row fallback therefore renders the caption once, and retained raw tables
avoid rendering a discarded caption beforehand.

## Validation after implementation

- `cargo test --manifest-path rust/Cargo.toml --test markdown -- --nocapture`:
  **47 passed, zero failed, two optional diagnostics ignored** (49 total).
- `cargo test --manifest-path rust/Cargo.toml --lib markdown::tests -- --nocapture`:
  both the caption-only regression and depth-200 render-once regression passed.
- The math worker reports **8/8 math_safety tests passed**, including all 18
  original/extracted/CommonMark-reparsed source contracts in default, debug, and
  standalone modes.
- `rustfmt --edition 2024 src/markdown.rs tests/markdown.rs` completed (paths run
  from the repository root used the `rust/` prefix).

The new regressions also cover malformed tag names, both quote characters in
attribute names, safe links remaining literal inside retained content, exact
parsed pre/code source across blank lines and tabs, SVG `viewBox` and namespaced
`xlink:href`, and exact nested inert-script source. An initial SVG assertion used
an unqualified attribute lookup; it was corrected to inspect namespace plus local
name, and the final integration run above passed.

Math conversion remains excluded. The exact unsupported-math warning is
unchanged. Retained Markdown bytes intentionally change where structural literal
serialization is required. Full corpus/snapshot review and final integrated
validation remain with the coordinating agent. No benchmarks were run.

The coordinating agent accepted the safe behavior for nonempty iframe fallback
text: character references do not decode inside iframe raw text. No visible
fallback or product attributes were added. This limitation is separate from the
exact pre/code and inert math-script source preservation verified above.
