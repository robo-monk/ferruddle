# Defuddle for Rust

Extract the main content of a web page as clean HTML or Markdown, with its title,
author and other metadata. A native Rust port of
[Defuddle](https://github.com/kepano/defuddle), with no browser or JavaScript runtime.

Give it HTML you have already fetched. Defuddle removes navigation, ads and other
clutter while preserving articles, code, lists, tables, images and footnotes.

## Use

Requires Rust 1.88 or newer. Until the first crates.io release, use the repository:

```toml
[dependencies]
defuddle = { git = "https://github.com/robo-monk/defuddle" }
```

```rust
use defuddle::{extract, Options};

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

From a checkout:

```sh
cargo run --release --manifest-path rust/Cargo.toml -- page.html --markdown
cat page.html | cargo run --release --manifest-path rust/Cargo.toml -- --json
```

Add `--url https://example.com/article` for relative links and site context.

## Scope

Defuddle works on static HTML, including content already embedded in the page.
It does not render JavaScript or retrieve remote content.

**Math conversion is not included.** Detected math produces the warning
“Math support is not yet included.” Safe source material can remain in output.

The port is checked against 226 real upstream HTML cases. Two local benchmark
runs measured HTML extraction at 7.29–7.48× and HTML plus Markdown at 7.38–7.61×
the speed of TypeScript under Vitest/linkedom. See the
[benchmark report](https://github.com/robo-monk/defuddle/blob/main/rust/docs/benchmarks/README.md)
for the method, raw results and host limits.

[API and options](https://github.com/robo-monk/defuddle/blob/main/rust/docs/api.md) ·
[Compatibility](https://github.com/robo-monk/defuddle/blob/main/rust/docs/scope.md) ·
[MIT license](LICENSE)
