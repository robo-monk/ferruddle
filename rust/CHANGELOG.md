# Changelog

## 0.1.0 — Unreleased

First native Rust port of Defuddle 0.19.5.

- Extract main content as cleaned HTML and optional Markdown, with metadata and
  debug information.
- Convert supplied HTML directly to Markdown.
- Support static shadow/streaming content and offline site adapters.
- Sanitize every extraction path and preserve rich code, lists, tables,
  footnotes, callouts and media.
- Detect unsupported math and return a structured warning. Math conversion is
  not included.
- Include a CLI and a pinned corpus of 226 real upstream HTML instances.

See [scope](https://github.com/robo-monk/defuddle/blob/main/rust/docs/scope.md)
for compatibility limits.
