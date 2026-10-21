# Ferruddle

Extract the main content from HTML as clean HTML or Markdown, with its title,
author and other metadata. Ferruddle is a native Rust port of
[Defuddle](https://github.com/kepano/defuddle).

Give it HTML you have already fetched. Ferruddle removes navigation and other
clutter while preserving articles, code, lists, tables, images and footnotes.
No browser or JavaScript runtime is required.

## Use

Requires Rust 1.88 or newer. Until the first crates.io release, use the repository:

```toml
[dependencies]
ferruddle = { git = "https://github.com/robo-monk/ferruddle" }
```

```rust
use ferruddle::{extract, Options};

let html = "<article><h1>Example</h1><p>A useful article paragraph.</p></article>";
let result = extract(
    html,
    Some("https://example.com/article"),
    &Options { markdown: true, ..Options::default() },
).unwrap();

println!("{}", result.content); // Cleaned HTML
println!("{}", result.content_markdown.as_deref().unwrap());
```

`content` always contains HTML. Requesting Markdown adds `content_markdown`.
The URL resolves relative links and selects site rules; it does not fetch the page.
Use `html_to_markdown(html, url)` to convert supplied HTML without article selection.

## CLI

```sh
cargo install --git https://github.com/robo-monk/ferruddle --locked ferruddle
ferruddle page.html --url https://example.com/article --markdown
cat page.html | ferruddle --json
```

## Scope

Ferruddle works on static HTML, including content already embedded in the page.
It does not render JavaScript or retrieve remote content.

**Math conversion is not included.** Detected math produces the warning
“Math support is not yet included.” Safe source material can remain in output.

The library is checked against 226 real upstream HTML cases. The extraction code
has measured speedups of 7.29–7.48× for HTML and 7.38–7.61× for HTML plus Markdown
against TypeScript under Vitest/linkedom. See the
[benchmark report](https://github.com/robo-monk/ferruddle/blob/main/docs/benchmarks/README.md)
for the method, raw results and host limits.

[API and options](https://github.com/robo-monk/ferruddle/blob/main/docs/api.md) ·
[Compatibility](https://github.com/robo-monk/ferruddle/blob/main/docs/scope.md) ·
[MIT license](LICENSE)
