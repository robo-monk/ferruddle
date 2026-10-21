# Port contract

The accepted scope is a native, synchronous, offline Rust library under `rust/`.
It accepts HTML and an optional page URL. `content` always contains cleaned HTML;
requesting Markdown adds `content_markdown`.

Math conversion is excluded. Detect math in original markup and in content
materialized from schema or adapters. Return one warning with code
`math_not_supported` and message `Math support is not yet included.` Preserve safe
source material where practical. Do not include a math conversion dependency.

Target extraction quality rather than a particular DOM serializer's bytes. Keep
source-backed improvements such as preserved code, link destinations, list
hierarchy, and footnote continuations. The default corpus gate compares rendered
semantics; every accepted difference has an explicit case ID, explanation, and
fixed fingerprints. Runtime code must not contain fixture-specific exceptions.

Sanitization is unconditional, including cleanup disabled, retry, schema, adapter,
and selected-root paths. Determine unsafe root identity before any retagging.
Traverse inert template content and foreign namespaces. Build page-derived HTML
through DOM setters or escape interpolated values at the point of construction.

The pinned reference is Defuddle 0.19.5 at revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. Original TypeScript source, fixtures,
and expectations remain unchanged. The portable corpus carries hashes and the
upstream MIT license. See [quality contract](quality.md) for acceptance evidence.

Performance comparisons must use identical input strings and URLs, equivalent
cleanup options, no network, fresh DOM parsing, and both HTML and HTML+Markdown
modes. Exclude unsupported math cases from both implementations. Save raw samples,
source hashes, environment details, warmup policy, and reproduction commands.

## Runtime coverage

The pipeline includes static shadow-root and React streaming recovery, lazy
images, rich code, footnotes, callouts, tables, lists and media. Offline site
adapters handle content present in the DOM or embedded JSON, including social
posts, discussions, GitHub, Substack, mail, conversations and video metadata.

Browser layout, JavaScript rendering, HTTP retrieval, network-only oEmbed/C2Wiki
extraction and remote captions are outside this library. YouTube supports supplied
DOM transcripts and embedded metadata; some upstream chapter and speaker
heuristics are not implemented. Site dispatch retains upstream domain matching.

The HTML5 parser and mutable selector tree use `dom_query`/`html5ever`; URL parsing
uses `url`, matching uses `regex`, and structured data uses `serde`. A focused
Markdown renderer shares the sanitized DOM. No JavaScript runtime or formula
conversion dependency is embedded.

The port is based on the upstream MIT-licensed project and retains its license.
It aims for equivalent extraction quality, with reviewed semantic differences,
rather than exact HTML serialization. It is an independent Rust implementation,
not a published release of the upstream TypeScript package.
