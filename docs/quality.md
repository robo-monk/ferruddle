# Quality contract and evidence

Reference: Defuddle 0.19.5, revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. The Rust implementation targets
extraction quality, with math conversion excluded.

## Corpus result

| Check | Result |
| --- | --- |
| Pinned, hashed upstream HTML instances | 226 |
| Matching title, author, site, published date, and extractor type | 226 |
| Cases with detected math and a required warning | 18 |
| Non-math cases | 208 |
| Exact reference Markdown | 165 |
| Matching rendered Markdown semantics | 189 |
| Additional source-reviewed Markdown results | 19 |
| Matching reference HTML semantics | 181 |
| HTML differences with explicit reasons, including unsupported math | 45 |
| Native HTML semantic regression snapshots | 226 |
| Readable native semantic/metadata snapshots | 226 |
| Reference prose preserved in HTML and rendered Markdown | 226 |
| Case-specific original math-source preservation contracts | 18 |

All 208 non-math cases meet either the reference semantic contract or a named,
reviewed contract. The 18 math cases require the exact warning, original source
values and multiplicities in HTML and rendered Markdown, surrounding prose, and
native regression snapshots. They are not reported as math conversion passes.

The portable bundle contains the actual upstream input HTML, its exact test URL,
reference HTML/Markdown, metadata, hashes, and revision provenance. The oracle was
exported only after every original golden passed. The unchanged TypeScript suite
passed all 826 tests with both linkedom and JSDOM during baseline research; see
the [baseline record](archive/research/rust-port-baseline.json).

The nested YouTube HTML has separate [provenance](../tests/data/youtube/manifest.json).
Its offline test verifies available metadata and embed output. A caption response
mocked by the source test is absent from that HTML, so remote transcript coverage
is not claimed.

## What the gate checks

`cargo test --locked` reads **every** manifest case.
It rejects missing, duplicate, altered, or unknown cases. It verifies metadata,
HTML semantic fingerprints, and rendered Markdown. It reparses cleaned HTML and
GFM-rendered Markdown and checks for active scripts, event attributes, iframe
`srcdoc`, navigation/resource directives, and unsafe URL schemes, including
responsive image candidates.

The semantic comparator preserves code bytes, language labels, link and image
destinations, responsive image sources, list nesting and numbering, task state,
table structure and spans, meaningful SVG whitespace, and the footnote target
graph. It tolerates serializer attribute order and narrowly defined wrapper or
spacing differences. Plain text comparison is diagnostic only.

Reviewed Markdown differences are stored in
[reviewed.json](../tests/data/quality/reviewed.json), unsupported math contracts in
[math.json](../tests/data/quality/math.json), and HTML contracts in
[html.json](../tests/data/quality/html.json). A reviewed difference has an exact ID,
reason, reference fingerprint, and native fingerprint. Obsolete or unknown
exceptions fail. Runtime code has no fixture ID exceptions.

Readable [native snapshots](../tests/data/quality/native) retain normalized HTML and
Markdown structure, metadata, warnings, word counts, and extractor type. Review
their visible diffs before changing fingerprints. The prose gate additionally
checks token multiplicities with block/cell boundaries; it excludes code (which
has exact structural checks) and unsupported formula representations. It has no
blanket allowed-loss exceptions. The gate failed on the earlier KaTeX
data-math case because its `Quadratic Formula` heading was lost, and passes after
the source-preservation fix.

The source contracts identify each math representation and compare all original
source signatures, including duplicates, against cleaned HTML and rendered
Markdown in default extraction, debug extraction, and standalone conversion.
These cover 128 structured/image/script sources plus authored raw TeX.
MathML annotation encodings and literal source text remain intact. Formula
conversion is excluded.

Only title, author, site, published date, and extractor type have upstream corpus
goldens. Other metadata fields have focused tests and native regression snapshots;
they are not claimed as reference-corpus parity checks.

Word counts are tested as logical content counts, rather than compared exactly
with TypeScript's tag-stripping count. The counter does not divide words at inline
wrappers; standardization retains upstream separator insertion. Paragraphs, table
cells, and other structural blocks divide words.

## Reviewed Markdown differences

The JSON contracts contain full IDs and fingerprints. These explanations were
checked against the original HTML by a separate reviewer.

| Case | Reason |
| --- | --- |
| Chroma line numbers | Retain the original C++ lambda deleted by TypeScript's empty-link cleanup. |
| ChatGPT CodeMirror | Retain an authored blank line. |
| Lazy image article | Do not infer C from an unrelated CSS class on an unannotated shell snippet. |
| ChatGPT citations | Retain strong emphasis inside authored headings. |
| Footnotes: heading notes | Retain the authored Notes heading. |
| Footnotes: HR continuation | Keep lists and continuation paragraphs inside the note definition. |
| Footnotes: HR strong numbered | Keep supporting list items inside the note definition. |
| Footnotes: HR superscript numbered | Retain the source divider before notes. |
| Footnotes: nested prose | Retain the source divider before notes. |
| Footnotes: pulldown definition | Retain the source divider before notes. |
| Footnotes: Word references | Retain a divider before notes. |
| Maggie Appleton Xanadu | Remove two stray spaces before periods after subscripts. |
| Daring Fireball | Omit a terminal divider before notes. |
| GitHub issue 56 | Retain a literal trailing space in the source code. |
| LessWrong article | Preserve literal inline-code text and the source nested-list sibling depths. |
| Noscript galleries | Preserve image-credit link destinations. |
| 12 Grams article | Preserve seven image links to full-size images. |
| Table cell header scoring | Preserve a preformatted block inside its table cell using safe HTML. |
| Peripheral tables | Preserve distinct cell paragraphs and alignment using safe HTML. |

The additional HTML-only differences concern wrappers, break-boundary whitespace,
legacy font repair, code-wrapper indentation, a decorative empty icon, normalized
backlink spacing, explicit empty alt text, and promotion of a real lazy-image
fallback. Each differing HTML snapshot has its own reason.
The X article also omits body meta directives unconditionally, while the reference
retains inert meta tags. This deliberate safety difference accounts for the
additional HTML deviation after the safety fixes.

## Independent checks

Focused tests cover DOM ownership and mutation, inert templates, declarative
shadow roots, streamed HTML, lazy images, poisoned attributes, SVG/MathML,
metadata, adapters, retries, schema fallback, code, tables, lists, footnotes, and
callouts. The independent review found and fixed real defects that corpus text
comparison missed, including unsafe-root retagging, unsafe metadata image
upgrades, distinct-image loss, lost base context, invalid nested paragraphs,
Markdown marker creation, lost code siblings, and rich-table structure loss.

Regression tests demonstrated the extraction and structural failures before the
fixes and pass afterward. Original TypeScript expectations were not changed.
Clippy with warnings denied, formatting, the public API example, and CLI smoke
checks are also part of final verification.

The [quality review](archive/reviews/IMPROVEMENT_PLAN.md) led to regressions for Unicode
date panics, unsafe schema-root fallback, unrelated-host social-profile matching,
exponential inline rendering, code breaks, directive sanitization, and math-source
loss. The [reviewed output changes](archive/reviews/hardening-quality-changes.json) record
why 12 native snapshots changed. Original HTML instances and TypeScript
expectations remain unchanged. Private helper tests run inside the crate under
`tests/internal`; public API tests remain normal integration tests.

A second review added rendered-DOM regressions for literal Markdown inside all
retained HTML families, malformed element/attribute names, blank lines in table
code, and block math followed by prose. One structural serializer now validates
names and escapes DOM values at emission. MathJax processing-control wrappers
produce normal Markdown without a false warning. These fixes preserve all 226
reviewed semantic snapshots. Independent, isolated normal/debug comparisons
also verify unchanged output for the later standardization and lazy-schema
performance edits.

The final renderer uses entities for literal tag-like text and explicit safe
autolinks. Three outputs consequently differ in Markdown bytes from the prior
168-exact checkpoint, with identical rendered semantics. No snapshot or
exception was added for these spelling changes. Hidden retries now search only
body descendants and keep the first equal-sized candidate, matching TypeScript;
anonymous fixtures and separate shadow/stream probes verify both rules.

To regenerate a diagnostic report without updating any contract:

```bash
cargo test --lib tests::corpus::report_full_pipeline -- --ignored --nocapture
```

This writes `/tmp/ferruddle-quality.json`. Fix defects before changing native
fingerprints. A legitimate change needs an input-backed explanation and review;
changing expectations to accommodate lost content is not an accepted workflow.

Aggregate counts are also available in [quality-report.json](quality-report.json).
