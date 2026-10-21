# Ferruddle

Keep `README.md` short and useful to library users. Put detailed documentation
in `docs/` and keep this index current:

- [API and options](docs/api.md)
- [Scope and compatibility](docs/scope.md)
- [Quality contracts and upstream fixtures](docs/quality.md)
- [Pinned TypeScript comparison](docs/reference-bridge.md)
- [Final benchmark report](docs/benchmarks/README.md)
- [Benchmark method and reproduction](docs/benchmarks/methodology.md)
- [Development and verification](docs/development.md)
- [Release procedure](docs/releasing.md)
- [Release verification](docs/verification.json)
- [Historical research and review records](docs/archive/README.md)

Run commands from this directory unless the documentation says otherwise:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

Preserve the upstream HTML instances, hashes and expectations. For extraction
fixes, add anonymous minimal fixtures under `tests/fixtures/` with expected
output under `tests/expected/`. Prove failure before and success after the fix.
Review semantic diffs before changing any native snapshot or contract; never
accept lost content by updating expectations.

Sanitization is unconditional, including disabled cleanup, selected roots,
schema, adapters and retries. Use DOM setters or escape page values at emission.
Strip `javascript:`, `blob:` and non-image `data:` URLs from link/source
attributes. Allow image data where supported, except for iframe sources.
Remove iframe `srcdoc` and every `on*` event handler. Parsing must not execute
scripts or load resources.
Protect literal page text when Markdown is rendered back into HTML. Cover new
adapter behavior with poisoned attributes in the Rust regression tests.

Math conversion remains excluded. Detection returns one warning with code
`math_not_supported` and message `Math support is not yet included.` Preserve
safe source where practical; do not add a formula conversion dependency.

Benchmarks must use the same inputs, URLs, cleanup options and fresh parsing,
without network. Keep unsupported math out of both timed implementations.
Never edit saved raw measurements or replace their recorded hashes with current
ones. Use a new output directory for each measurement, separate baseline/current
Cargo targets, and retain source/input identity and host monitoring.

Publication is a manual maintainer action described in `docs/releasing.md`;
no CI job uploads a crate. Keep the repository root a standalone Rust crate.
TypeScript comparison sources are reconstructed from the pinned archive under
the ignored `.benchmarks/` directory; they are not application source.

The README targets users. Keep installation, a small example, CLI usage and real
limits there. Put implementation notes, validation, research and detailed
benchmarks in `docs/`, with links in this index.
