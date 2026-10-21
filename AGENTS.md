# Ferruddle

Keep the root a standalone Rust crate. Write the README for library users;
put detailed guidance in `docs/` and keep the links below current.

Run commands from this directory:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Preserve upstream HTML, hashes and expectations. For extraction fixes, add
anonymous minimal fixtures in `tests/fixtures/` and expected output in
`tests/expected/`. Prove failure before and success after the fix. Review
semantic diffs before changing snapshots or contracts; never accept lost content.

Sanitization is unconditional across options, selected roots, adapters, schema
and retries. Use DOM setters or escape page values at emission. Strip
`javascript:`, `blob:` and non-image `data:` URLs; forbid all iframe `data:` URLs.
Remove iframe `srcdoc` and every `on*` handler. Parsing must not execute scripts
or load resources. Protect literal page text when Markdown is rendered as HTML.
Cover adapter behavior with poisoned attributes.

Exclude math conversion dependencies. Detection returns one warning:
`math_not_supported` / `Math support is not yet included.` Preserve safe source
where practical.

Use identical benchmark inputs, URLs, cleanup options and fresh parsing without
network. Exclude math from both timed implementations. Never edit saved raw
measurements or their hashes. Use new output directories, separate baseline and
current Cargo targets, source/input identity and host monitoring.

Publication is a manual maintainer action. No CI job uploads a crate. Reconstruct
TypeScript comparison sources under ignored `.benchmarks/`; keep them outside
the maintained application source.

## Documentation

- [API and options](docs/api.md): check public behavior and CLI arguments.
- [Scope and compatibility](docs/scope.md): check offline coverage and upstream differences.
- [Quality contracts](docs/quality.md): read before changing extraction or expectations.
- [Development](docs/development.md): find modules, invariants and diagnostic commands.
- [Release procedure](docs/releasing.md): validate, package and publish a release.
- [Benchmark report](docs/benchmarks/README.md): interpret results and host limits.
- [Benchmark method](docs/benchmarks/methodology.md): prepare the reference and reproduce measurements.
- [Benchmark evidence](docs/benchmarks/evidence.md): locate raw data, source archives and verifiers.
