# API and options

The two public entry points accept UTF-8 HTML and an optional page URL. They are
synchronous, perform no network requests and return owned values. Independent
calls can run on separate threads. No JavaScript executes and no resources load.

```rust
use defuddle::{extract, html_to_markdown, Options};

let html = "<article><h1>Example</h1><p>A useful paragraph.</p></article>";
let page = extract(html, Some("https://example.com/article"), &Options {
    markdown: true,
    ..Options::default()
}).unwrap();

assert!(!page.content.is_empty());
assert!(page.content_markdown.is_some());

let fragment = html_to_markdown("<p>Hello <strong>world</strong>.</p>", None).unwrap();
assert_eq!(fragment.markdown, "Hello **world**.");
```

## Extraction

`extract(html, page_url, options)` selects the main content, cleans it, extracts
metadata and optionally renders Markdown. `Extraction` contains:

| Field | Meaning |
| --- | --- |
| `content` | Cleaned HTML, including when Markdown is requested |
| `content_markdown` | Markdown when `options.markdown` is true |
| `metadata` | Title, author, description, site, publication date, language, domain, image, favicon and schema.org data |
| `word_count` | Count over the cleaned content, with structural block boundaries |
| `extractor_type` | Name of a selected site adapter, when used |
| `warnings` | Structured warnings, including unsupported math |
| `debug` | Content selector, removals and diagnostic errors when requested |

Rust fields use snake case. Serde JSON uses camel case and flattens metadata into
the result object. Empty metadata strings indicate unavailable values.
Metadata remains untrusted page data: escape text and validate URLs for their
destination before using it in another application.

The word counter does not split a word merely because an inline wrapper divides
it. Standardization retains upstream spacing behavior and can insert separators.

## Options

| Option | Default | Behavior |
| --- | --- | --- |
| `markdown` | `false` | Add a separate Markdown result |
| `content_selector` | `None` | Select a content root; invalid or unmatched selectors use automatic selection |
| `remove_exact_selectors` | `true` | Remove known clutter selectors |
| `remove_partial_selectors` | `true` | Remove clutter identified by attributes |
| `remove_hidden_elements` | `true` | Remove hidden content, with recovery retries |
| `remove_low_scoring` | `true` | Remove blocks scored as clutter |
| `remove_small_images` | `true` | Remove icons and unresolved tiny placeholders |
| `remove_content_patterns` | `true` | Remove repeated metadata, navigation, related cards and subscription widgets |
| `remove_images` | `false` | Remove images, including images materialized by adapters |
| `standardize` | `true` | Normalize headings, code, images, tables, footnotes, callouts and wrappers |
| `include_replies` | `Extractors` | Include available local replies; `None` excludes them |
| `debug` | `false` | Return diagnostics and preserve diagnostic attributes |

`IncludeReplies::All` currently selects the same offline replies as `Extractors`,
matching the source behavior. No option enables fetching replies.

Sanitization runs with every option combination, including schema and adapter
paths. It removes executable scripts, unsafe selected roots, event handlers,
dangerous URL schemes, `meta`/`link` directives and iframe `srcdoc`. DOM parsing
and serialization escape page-derived text and attributes.

## Standalone Markdown

`html_to_markdown(html, page_url)` converts the supplied HTML without selecting
main content. It returns `MarkdownOutput { markdown, warnings }`. It includes
static declarative shadow content and omits a leading H1 article title to match
the extraction dialect.

Both APIs resolve relative links using the page URL and an authored `<base href>`.
Missing or invalid page URLs leave relative destinations as authored. The URL
provides context and site rules; it does not cause a request.

## Math warning

Detected MathML, KaTeX, MathJax or TeX returns one warning:

```json
{"code":"math_not_supported","message":"Math support is not yet included."}
```

Safe original source can remain in output. Formula conversion and full math
rendering are outside the current scope.

## CLI

```sh
cargo run --release --manifest-path rust/Cargo.toml -- page.html --url https://example.com/article --markdown
cat page.html | cargo run --release --manifest-path rust/Cargo.toml -- --markdown --json
```

Run these commands from the repository root. The CLI reads a UTF-8 file or stdin
(`-` is accepted). Plain mode writes cleaned HTML by default, or Markdown with
`--markdown`; warnings go to stderr. `--json` writes the complete extraction,
including warnings, to stdout. `--url` supplies page context without fetching.

Use `--help` for the command synopsis. Invalid arguments, unreadable input and
extraction errors produce a nonzero exit status.
