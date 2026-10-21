# Scope and compatibility

Ferruddle is a native, synchronous Rust library for supplied UTF-8 HTML and an
optional page URL. It returns cleaned HTML, metadata and optional Markdown.
It targets Defuddle's extraction quality, with reviewed semantic differences
rather than exact HTML serialization.

The pinned reference is Defuddle 0.19.5 at revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. The
[upstream corpus](../tests/data/upstream/manifest.json) preserves original HTML,
URLs, expectations, hashes and provenance. The
[quality contracts](quality.md) define acceptance. Runtime code contains no
fixture-specific exceptions.

## Supported content

The pipeline handles static declarative shadow roots, React streaming markup,
lazy images, rich code, footnotes, callouts, tables, lists and media. Offline
site adapters use the DOM or embedded JSON for social posts, discussions,
GitHub, Substack, mail, conversations and video metadata.

The [API reference](api.md) describes cleanup options, local replies, URL
resolution and standalone Markdown conversion. Sanitization runs regardless
of cleanup options. Metadata remains untrusted page data.

## Limits

Ferruddle performs no HTTP retrieval, JavaScript execution or browser layout.
It cannot retrieve network-only oEmbed or C2Wiki content, remote replies or
captions. YouTube extraction uses supplied DOM transcripts and embedded
metadata; some upstream chapter and speaker heuristics are absent.
Site dispatch retains upstream domain matching.

Inputs are Rust UTF-8 strings. JavaScript inputs containing lone UTF-16
surrogates have no direct Rust string equivalent.

Math conversion is excluded. Detection covers original markup and sources
materialized by adapters or schema. It returns one warning with code
`math_not_supported` and message `Math support is not yet included.` Safe source
can remain in the output; this does not provide formula rendering.

Word counts describe cleaned content with structural block boundaries. They
can differ from the reference's tag-stripping count. Standalone Markdown
omits a leading H1 article title, matching the extraction dialect.

## Implementation and license

`dom_query` supplies an HTML5 parser and a mutable DOM with selectors.
Ferruddle uses `url` for URLs, `regex` for matching, `serde` for structured data
and a Markdown renderer over the sanitized DOM. Builds require no JavaScript
runtime or formula conversion library.

Ferruddle is an independent Rust port of the MIT-licensed Defuddle project.
It retains the [upstream license](../LICENSE).
