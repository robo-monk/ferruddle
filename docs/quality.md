# Quality contracts

The pinned reference is Defuddle 0.19.5, revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. Ferruddle targets equivalent
extraction quality. Math conversion is excluded.

## Corpus coverage

| Check | Cases |
| --- | ---: |
| Pinned, hashed upstream HTML instances | 226 |
| Matching title, author, site, publication date and extractor type | 226 |
| Non-math cases | 208 |
| Exact reference Markdown among non-math cases | 165 |
| Matching rendered Markdown semantics among non-math cases | 189 |
| Additional reviewed Markdown differences | 19 |
| Matching reference HTML semantics | 181 |
| Reviewed HTML differences, including unsupported math | 45 |
| Native HTML and readable semantic snapshots | 226 |
| Reference prose preserved in HTML and rendered Markdown | 226 |
| Original math-source contracts with a required warning | 18 |

All 208 non-math cases meet the reference semantic contract or a named, reviewed
contract. The math cases require source preservation, surrounding prose and the
exact warning. They do not establish formula conversion support.

The [upstream bundle](../tests/data/upstream/manifest.json) contains actual HTML,
exact test URLs, reference HTML and Markdown, metadata, hashes and provenance.
The nested YouTube input has separate
[provenance](../tests/data/youtube/manifest.json). It covers supplied metadata and
embed output; a remote caption response is absent from the HTML.

Only title, author, site, publication date and extractor type have upstream
corpus goldens. Other metadata fields have focused tests and native snapshots.
Word counts use logical content boundaries, not the reference's tag-stripping
count. Inline wrappers do not divide words; structural blocks and cells do.
Standardization can insert separators.

## Acceptance checks

`cargo test --locked` reads every manifest case. It rejects missing, duplicate,
altered or unknown cases. The gate checks metadata, HTML structure, rendered
Markdown, prose preservation and native regression snapshots.

The semantic comparator preserves code bytes, language labels, link and image
destinations, responsive image sources, list nesting and numbering, task state,
table structure and spans, meaningful SVG whitespace and footnote targets.
It tolerates attribute order and narrowly defined wrapper or spacing differences.
Plain text comparison is diagnostic only.

The gate reparses cleaned HTML and GFM-rendered Markdown. It checks active
scripts, event attributes, iframe `srcdoc`, navigation and resource directives,
unsafe URL schemes and responsive image candidates.

The prose check uses token multiplicities and block/cell boundaries. It excludes
code, which has structural checks, and unsupported formula representations.
It has no blanket exception for lost prose.

## Reviewed differences

Contracts hold the case IDs, reasons and reference/native fingerprints:

- [Markdown differences](../tests/data/quality/reviewed.json).
- [HTML contracts](../tests/data/quality/html.json).
- [Math-source contracts](../tests/data/quality/math.json).
- [Readable native snapshots](../tests/data/quality/native).

The differences preserve source content such as code, image links, footnote
continuations and rich table structure. Other reasons cover serializer wrappers,
whitespace and unconditional directive removal. Obsolete and unknown exceptions
fail the gate. Runtime code contains no fixture ID exceptions.

Math contracts compare source signatures and multiplicities in default
extraction, debug extraction and standalone conversion. They cover 128
structured, image and script sources, plus authored raw TeX. MathML annotations
retain their encodings and literal text. Detection returns one
`math_not_supported` warning with message `Math support is not yet included.`

## Focused regressions

Public API tests and crate-local tests cover DOM mutation, inert templates,
shadow roots, streaming markup, lazy images, poisoned attributes, SVG/MathML,
metadata, adapters, retries, schema fallback, code, tables, lists, footnotes and
callouts. Markdown tests check literal text after rendering retained HTML,
malformed names, blank lines in table code and prose after block math.
Deep inline tests check bounded render work and token preservation without
wall-clock assertions.

For an extraction fix, add an anonymous minimal input under `tests/fixtures/`
and expected output under `tests/expected/`. Prove failure before the fix and
success afterward. Keep upstream HTML, expectations and hashes unchanged.
Review visible semantic diffs before changing any native fingerprint; never
accept content loss by updating expectations.

For diagnostics without changing contracts, run:

```sh
cargo test --lib tests::corpus::report_full_pipeline -- --ignored --nocapture
```

The command writes `/tmp/ferruddle-quality.json`. See
[development](development.md) for source layout and verification commands.
