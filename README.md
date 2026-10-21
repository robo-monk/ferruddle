# Ferruddle

Ferruddle extracts the main content from HTML as clean HTML or Markdown.
It is a native Rust port of [Defuddle](https://github.com/kepano/defuddle).

## Use

Use Rust 1.88 or newer. Add the Git dependency to `Cargo.toml`:

```toml
[dependencies]
ferruddle = { git = "https://github.com/robo-monk/ferruddle" }
```

```rust
use ferruddle::{extract, Options};

let page = extract(
    "<article><h1>Example</h1><p>A useful paragraph.</p></article>",
    Some("https://example.com/article"),
    &Options { markdown: true, ..Options::default() },
).unwrap();

println!("{}", page.content);
println!("{}", page.content_markdown.unwrap());
```

`content` contains cleaned HTML. Set `markdown: true` to also get `content_markdown`.
The result includes metadata such as the title and author.

Supply HTML that you have fetched. The optional URL resolves relative links and
selects site rules. Ferruddle does not fetch pages or run JavaScript.

To convert HTML without selecting its main content, use `html_to_markdown(html, url)`.
See the [API and options](https://github.com/robo-monk/ferruddle/blob/main/docs/api.md).

## CLI

Install the CLI:

```sh
cargo install --git https://github.com/robo-monk/ferruddle --locked ferruddle
```

Extract Markdown from a file, or read HTML from stdin:

```sh
ferruddle page.html --url https://example.com/article --markdown
cat page.html | ferruddle --json
```

## Limits

Ferruddle uses content present in the supplied HTML. It cannot recover content
that requires JavaScript or a network request.

Math conversion is excluded. Detected math returns the warning
`Math support is not yet included.` Safe source can remain in the output.

Read the [compatibility notes](https://github.com/robo-monk/ferruddle/blob/main/docs/scope.md),
[quality contracts](https://github.com/robo-monk/ferruddle/blob/main/docs/quality.md),
and [benchmarks](https://github.com/robo-monk/ferruddle/blob/main/docs/benchmarks/README.md) for details.

Licensed under [MIT](LICENSE).
