# Hardening correctness review, round 1

Reviewer: Opus 5.5 (xhigh), read-only. Date: 2026-10-21.

## Scope and method

- Read `AGENTS.md`, the improvement plan, the three original Opus reviews,
  `root-verification.md`, and the three hardening responses.
- Read the changed runtime in full: `markdown.rs`, `pipeline.rs`, `math.rs`,
  `safety.rs`, `finalize.rs`, `dom/selectors.rs`, and `utils.rs`. Read the
  relevant parts of `standardize.rs`, `removals.rs`, `selection.rs`,
  `metadata.rs`, and the adapters.
- Reviewed against a hash-verified copy of `rust/` (46 source/test/Cargo
  files). It was rechecked at 00:15 and matched the working tree, including
  the root's later `metadata.rs` fix.
- Built probes with toolchain 1.99.1 in separate targets under
  `~/.cache/defuddle-port-hardening/opus-review/`:
  - a current-tree probe (`target-probe-current`);
  - a baseline-source probe (`target-probe-baseline`);
  - a current-tree CLI (`target`).
- Probes call only the public `extract` and `html_to_markdown`. Markdown is
  re-rendered with pulldown-cmark 0.13 using all extensions. Inputs are in
  `opus-review/inputs/`.
- Not run: the full suite, fmt, Clippy, or benchmarks. I did not inspect the
  benchmark work or the native snapshots that the root is reviewing. I ran
  TypeScript (`dist/node.js` with linkedom) only for F1b.

## Findings, highest severity first

### F1 — High: retained raw HTML can still turn page text into active Markdown links

The renderer keeps some elements as raw HTML: math holders, `sup`/`sub`/`svg`,
media fallbacks, unusual lists, and complex tables. CommonMark still parses
Markdown syntax in the text between inline HTML tags. A blank line also ends an
HTML block. The hardening added `protect_retained_math_html`
(`markdown.rs:936`) for this reason, but the protection is incomplete.

**F1a: new code; I recommend fixing this before release.** The protector tracks tag
and quote state by scanning the serialized string. html5ever allows `'` and `"`
in attribute **names**, and the serializer emits them unchanged. Such a name
desynchronizes the scanner, which then treats the remaining text as tag content
and leaves it unprotected. CommonMark rejects the malformed tag and parses the
following text as Markdown.

These attribute names survive sanitization in three modes:

- `debug: true`, which keeps `data-*` attributes;
- `standardize: false`;
- `html_to_markdown`.

Repro (`html_to_markdown`):

```html
<p>See <span class="math"><b data-a'b="1">x</b> [evil](javascript:alert(1))</span> done.</p>
```

The rendered output contains `<a href="javascript:alert(1)">evil</a>`. The same
result occurs with `extract(markdown, debug)` on a `data-math` holder
(`inputs/quote-attr-article.html`). Existing tests cover poisoned attribute
**values** only (`tests/markdown.rs:752-775`).

**F1b: pre-existing and matches upstream; required for any Markdown-safety claim.**
Non-math retained HTML gets no protection at all:

- `sup`/`sub`/`svg` at `markdown.rs:287`;
- media at `:454`;
- lists at `:509`;
- complex tables at `:703`.

With default `extract(markdown: true)`, the input
`<sup>[click me](javascript:alert(1))</sup>` renders as an active
`javascript:` link. Baseline Rust and TypeScript 0.19.5 emit the same Markdown,
so this is not a regression. It does contradict the GFM-rendered safety check
described in `QUALITY.md` and the `assert_safe_markdown` standard in
`tests/markdown.rs`.

The same mechanism damages structure. In a colspan table, a `<pre>` containing a
blank line ends the HTML block. The indented line that follows becomes a code
block, and `</code></pre>…</table>` appears as literal text
(`inputs/table-blank.html`, default options). In standalone conversion, the
text after that break was parsed as Markdown, giving `javascript:alert(3)`.

**Fix.** Use one structural serializer for every retained-HTML path:

- walk the sanitized DOM;
- emit only CommonMark-valid attribute names, `[A-Za-z_:][A-Za-z0-9_.:-]*`;
- escape attribute values, including quotes and line breaks;
- encode text punctuation, newlines, and tabs as the math protector already does.

Add tests for:

- quote-bearing attribute names;
- link text in `sup`, `svg`, list, and table retention;
- a blank line inside a retained `pre` or table cell.

### F2 — Medium regression: the shared math predicate makes whole sections raw HTML

`math::is_math_source` (`math.rs:48-79`) uses the broad **detection** rules,
including class prefixes `mathjax*` and `katex*`, `latex`, and `role=math`. It
is now reused for Markdown retention (`markdown.rs:269`) and for standardize
protection (`standardize.rs:39-41`).

MathJax processing-control classes therefore count as formula source. A common
example is MyST/Jupyter Book output such as
`<section class="tex2jax_ignore mathjax_ignore">`.

Repro: `inputs/jupyterbook.html`.

- **Current `html_to_markdown`:** returns one line of raw
  `<section …>…</section>` HTML with `&#10;` entities. No headings, lists,
  links, or paragraphs are converted.
- **Baseline:** returns normal Markdown.
- **Also affected:** `extract` with `debug: true` or `standardize: false`.
- **Not affected:** default extraction, in my probe. Classes are stripped or the
  wrapper is flattened before rendering.

The prose gate cannot catch this, because every word survives.

**Fix.**

- Separate "page contains math" from "retain this element as source".
- Retain only explicit source holders and known generated roots, such as
  `math`, `mjx-container`, `.katex`, `.MathJax`, and the source attributes.
- Never retain an element that contains native prose blocks; render its
  children instead.
- Exclude `*_ignore` and `*_process` control classes from detection as well.
  The false warning on such pages already existed in baseline.

### F3 — Medium, partly a regression: a block-level math holder swallows the prose after it

`children()` appends a source-block child's output without blank-line
separation (`markdown.rs:157-160`). A retained `div` holder therefore starts a
line beginning with `<div`, which CommonMark treats as an HTML block. Any inline
prose that follows on that line stays raw.

Repro (default options, `inputs/div-math.html`):

```html
<div><div data-math="a+b"></div>Then *literal* star and [bracket] text with <em>emphasis</em> follows here.</div>
```

- **Current output:** `\*` and `\[` escapes appear literally, and the emphasis
  is not rendered.
- **Baseline:** dropped the empty holder, so the source was lost but the prose
  rendered correctly.

This applies to:

- `div[data-math]`, `div[data-tex]`, and `div[data-latex]` holders (the latter
  already in baseline);
- debug or standalone `div.math` (Sphinx) and `div.MathJax_Display`.

All corpus holders are `span` elements, so the 18 source contracts do not cover
this case. The token-based prose gate would also pass it.

**Fix.** Separate retained output from source-block nodes with blank lines
(`block()`), and add a test with prose before and after a block holder. The test
should assert the rendered `<em>` and the absence of literal backslash escapes.

### F4 — Low, already fixed: oversized `og:site_name` panicked in `clean_title`

I reproduced exit 101 (`CompiledTooBig`) for a 200–400 KB `og:site_name`.
Baseline panicked too. The root fixed it independently at 23:58 with
`let Ok(regex) … else continue` (`metadata.rs:724`) and added a regression test.

I verified the fix on the current tree: 400 KB and 4 MB inputs both exit 0. The
remaining cost is linear but heavy: 2.3 s and 21 s in a debug build. As an
optional follow-up, cap the site length before building patterns, or compare
without regex.

### F5 — Low/nit: the once-per-node render claim has one exception

`table()` first renders the caption's children (`markdown.rs:642`). With zero
rows (`width == 0`, `:705-706`), it then re-renders the whole table, including
the caption. The extra work is bounded at 2× (a caption containing a table takes
another branch), so this does not reintroduce D1.

However, a unit test of `<table><caption>x</caption></table>` would trip the
`#[cfg(test)]` assertion at `:174`. Note also that this assertion is compiled
only into crate-local unit tests. The depth-200 integration tests in
`tests/markdown.rs` guard only by terminating. Consider computing the caption
lazily.

## Optional observations

- **O1 (product decision):** Empty `data-math` holders keep their TeX only as an
  attribute. Rendered HTML and Markdown still read
  "For all , if is a man, then is mortal." A literal code-span fallback for
  empty holders would make the source visible without converting it. This
  matches the existing treatment of `script[type=math/tex]`.
- **O2 (performance watch for the benchmark agent):**
  - `standardize::protected()` now evaluates `closest(SOURCE_SELECTOR)`, a
    selector with about 25 alternatives, for every node at about ten call
    sites.
  - `prepare()` always runs `schema_element` (full clone plus sanitize) when
    JSON-LD has text. Baseline computed it only when the body or schema
    branch needed it.
- **O3 (test design):** The prose gate checks word multisets. It cannot detect
  raw-HTML-block swallowing or a whole-section HTML blob, because F2 and F3 keep
  every word. Add rendered-structure assertions: no literal Markdown escapes in
  rendered text, and preserved heading, paragraph, and list counts. Also run the
  math source contracts in debug and standalone modes.

## Checked, no defect found

- **D1 equivalence:** I ran 11,000 random inline trees through
  `html_to_markdown` on the baseline and current builds. Seeds 1–3 had no
  `br` or `div` inside code and were byte-identical. Seed 9 had 1,051
  differences. Every inspected difference came from `br` or `div` inside
  `code`/`tt`, the intended Q8 change. The 48 cases my regex did not classify
  involved nested code, and the four I inspected had the same cause.
  The 60-level punctuation trigger and 80 unclosed `font` items completed
  through default `extract` with Markdown in 0.96 s and 0.32 s (debug).
- **Pipeline:** Stage order, retry thresholds, hidden-retry length comparison,
  adapter HTML/selector paths, debug errors, and the schema branch match
  baseline. Selection and schema search do not mutate `prepared.original`.
  Arena IDs are clone-stable.
  - The single math scan differs from baseline only in a contrived case:
    React streaming content moved out of `pre`/`code` without image promotion.
- **Schema roots:** Pruning is sound because a node's text is a substring of
  its ancestor's text, so word counts are monotone. Ties keep the first match in
  document order. Search runs on a sanitized clone, and the fallback is intact.
- **Selector index:** `necessary_key` rejects combinators, lists, namespaces,
  escapes, non-ASCII input, and comments. dom_query matches in no-quirks mode
  and splits classes on ASCII whitespace, which agrees with the index buckets.
  The exact list uses no `:scope`.
- **Bounded byline counting:** The ancestor text is monotone, so `take_while`
  is equivalent to the baseline check.
- **Other areas:**
  - date parsing (ASCII, fallible, valid calendar dates);
  - social boundaries, equivalent to the TypeScript `intent\b` and `share\b`
    rules;
  - conversation control cleanup;
  - unconditional `meta`/`link` removal, including explicit roots;
  - re-sanitization after `<base>` and URL resolution, so a
    `javascript://x/` base is stripped.
- **Sanitizer placement:** It runs last on every output path, including
  explicit unsafe roots, adapter HTML, the schema fallback, and standalone
  conversion.
- **Test honesty:**
  - The 18 `sourceContract` entries compare multiplicities and MathML element
    order across the original HTML, native HTML, and rendered Markdown.
  - The prose-gate logs show the baseline failing only on
    `quadratic`/`formula`, and the current tree passing.

## Release decision from this review

| Status | Findings |
| --- | --- |
| Blockers | F1a and F2 (both new behavior); F3 (cheap fix) |
| Required before claiming Markdown output safe without a downstream sanitizer | F1b |
| Already fixed | F4 |
| Nit | F5 |
| Optional | O1–O3 |
