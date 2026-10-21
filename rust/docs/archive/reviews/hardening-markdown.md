# Markdown hardening

Implemented D1/Q3, Q8, and Q10 in `rust/src/markdown.rs`, with tests in
`rust/tests/markdown.rs` and the new `rust/tests/data/markdown/inline-code-breaks`
HTML/Markdown fixture pair. Upstream TypeScript, upstream fixtures, corpus
contracts, and snapshots were not edited by this worker.

## Rendering work and source boundaries

`children()` now keeps a lazily rendered child in its lookahead. Inline children
retain their kind and undecorated content until their run is complete. This is
necessary beyond caching the final string: a probed inline child may later start
its own same-kind merge, which otherwise renders that child's descendants again.
Both ordinary lookahead and this merge-start case now render each node at most
once. A test-only node-ID assertion fails immediately if any renderer repeats a
node, avoiding an unbounded regression test when exponential traversal returns.

Native block containment is computed once, bottom-up, in a node-ID set. Rendering
uses set membership instead of scanning every child's descendants. The same-kind
merge rules remain: empty nonblock nodes may be skipped; whitespace and authored
block-containing nodes stop the run; code content joins without inserting an
extra separator. Dedicated assertions cover all four boundaries. Specialized
math-source and callout rendering takes precedence over inline delimiter merging.

New regressions exercise 200 levels of nested inline wrappers, default extraction
of the punctuation trigger, realistic unclosed legacy font elements, and a
lookahead that becomes the start of another merged inline run. They assert token
multiplicity and preservation rather than elapsed time.

## Code, retained source, and standalone behavior

Inline code uses the existing source-text walker, which maps `br` to a newline;
code-span rendering then maps that newline to a space. Singleton and merged code
runs use this same path. The fixture covers paragraphs, tables, and merged
`code`/`tt` siblings, preventing `make` and `install` from becoming `makeinstall`.

The renderer shares `math::is_math_source` with cleanup. Safe `data-math`,
`data-tex`, `data-mathml`, and `data-katex` source holders are preserved as HTML,
and are excluded from inline delimiter merges. Poisoned attributes remain inert.
The previous complex-table handling of inert math scripts is factored into
`retained_html`: retained math holders and other raw-HTML fallbacks also replace
inert math script descendants with code elements before serialization. This
preserves literal source and prevents a later attribute filter from removing the
MIME type while leaving a script element. No formula conversion was introduced;
tests retain the exact `Math support is not yet included.` warning.

The stronger source gate then exposed another real loss: CommonMark interprets
Markdown syntax inside inline raw HTML. An annotation containing
`\mathrm{head}_{1},...,\mathrm{head}_{h}` survived reparsing only as
`\mathrm{head}`. In addition, multiline MathJax custom elements with blank lines
and indentation could leave their closing tags interpreted as code and swallow
later headings and prose into the math container. Retained HTML containing math
now protects literal text punctuation with numeric HTML references, and protects
physical newlines/tabs in text and quoted attributes. This preserves exact
reparsed source while emitting one physical line, including when the math is
nested in a retained table. Attribute/tag syntax is unchanged. New regressions
check exact TeX annotation text, absence of injected Markdown links/emphasis,
poisoned source attributes containing quotes/newlines, and following prose staying
outside a multiline MathJax container. This is source preservation, not formula
conversion; math Markdown bytes intentionally change where protection is needed.

Standalone conversion now flattens valid declarative shadow roots before
sanitization, including nested roots. Ordinary and invalid templates remain
inert. Relative destinations, image candidates, and retained HTML URLs use the
shared prepared base URL, including an authored `base href`; the sanitizer runs
again after URL resolution. Missing or invalid page URLs preserve authored
relative values. Fragment links remain fragments. Leading H1 title omission
remains source-compatible and is documented in code and tested explicitly.

The existing entity-safety test now covers both absent and present URL context:
`java&#x73;cript:alert(1)` remains literal, and with page context resolves beneath
`https://example.com/` without decoding a second entity layer. This is an intended
standalone URL behavior change, not a relaxed sanitizer expectation.

## Before/after evidence

The immutable baseline CLI is
`/home/robomonk/.cache/defuddle-port-hardening/defuddle-baseline`.
Inputs and JSON results are under
`/home/robomonk/.cache/defuddle-port-hardening/markdown-evidence/`.

| Case | Baseline | Current debug CLI |
| --- | --- | --- |
| 24-level punctuation inline trigger | Killed at a bounded 3-second timeout | Completed in 0.158 seconds |
| 24 unclosed font items with article context | Killed at a bounded 3-second timeout | Completed in 0.084 seconds |
| Inline code display break | ``Run `makeinstall` now.`` | ``Run `make install` now.`` |
| New code-break fixture | Paragraph and table tokens merge; does not match expected output | Exact expected Markdown |

These are bounded correctness reproductions, not isolated performance benchmark
claims; other hardening changes were integrated and other agents were active.
`before.json`, `after.json`, `before-inline-code-fixture.json`, and
`after-inline-code-fixture.json` preserve the raw evidence.

## Validation

Used the private Rust 1.99.1 binaries under
`/tmp/opus-perf/rustup-home/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin`, with
`RUSTC` and `RUSTDOC` set explicitly, `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`, and the requested shared current-source target directory.

- `cargo test --manifest-path rust/Cargo.toml --lib --test markdown -- --nocapture`:
  10 library tests and 36 Markdown integration tests passed; two optional external
  diagnostics remained ignored.
- After the additional raw-math source regressions,
  `cargo test --manifest-path rust/Cargo.toml --test markdown -- --nocapture`:
  38 passed, two optional diagnostics ignored, 3.62 seconds.
- The depth-200 merge-lookahead unit regression passed with the per-node
  assertion enabled; its standalone focused run took 0.04 seconds.
- `cargo test --manifest-path rust/Cargo.toml --test independent_review adjacent_ -- --nocapture`:
  both existing adjacent code/emphasis regressions passed.
- The math/safety worker subsequently ran all six `math_safety` tests successfully,
  including all 18 pinned source-contract cases comparing original HTML, extracted
  HTML, and CommonMark-reparsed Markdown by source/MathML tag order and multiplicity.

A shared-target baseline build from another worker temporarily contaminated the
current crate artifacts. A bounded code-break test exposed the stale behavior;
the accidentally linked baseline deep tests were terminated, the owned Markdown
source was touched to force a fresh current-source library build, and all checks
above were rerun successfully. Further baseline builds use a separate target.

Full corpus review, final formatting, Clippy, release checks, public API examples,
and the final benchmark are owned by the coordinating agent. No remaining
Markdown implementation issue was identified by the focused checks.
