# Releasing the Rust crate

The crate is prepared as `defuddle` 0.1.0 under the MIT license. Preparation does
not publish a crate, push a tag or create a hosted release. Rust publication is
manual; CI never uploads to crates.io.

The repository is `https://github.com/robo-monk/defuddle`. Use `rust-v0.1.0` as
the first Rust tag. The npm release workflow excludes `rust-v*` tags, and the
Rust workflow checks that a Rust tag agrees with `Cargo.toml`.

## Validate the candidate

Run from `rust/` on the intended release commit:

```sh
cargo test --locked
cargo +1.88.0 test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo package --locked --list
cargo publish --locked --dry-run
```

The package allowlist includes library/CLI source, examples, the complete tests
and fixtures, README, changelog, license and lockfile. It excludes local build
trees, raw benchmarks, review logs and research archives. The fixtures remain in
the package so the original quality gate can also run from an unpacked crate.
Inspect `target/package/defuddle-0.1.0.crate` and run its unpacked tests before
publication. Local checks on an uncommitted working tree need `--allow-dirty`;
the release commit and CI should not.

The benchmark verifier is independent of the package:

```sh
python3 docs/benchmarks/tools/verify.py
```

It checks preserved measurement inputs and raw results against the archived
source. Packaging metadata, Rust documentation and test-path cleanup happened
after measurement. Runtime behavior and locked dependencies are unchanged;
that relationship is checked explicitly rather than rewriting old source hashes.

## Publish

After the release commit passes Linux, macOS, Windows and minimum-version CI:

1. Confirm the crates.io name and ownership. `defuddle` was unregistered during
   preparation; availability is not a reservation. Authenticate through the
   normal Cargo workflow without storing a token in this repository.
2. Replace the README's temporary Git dependency with `defuddle = "0.1"`, date
   the changelog entry, commit those changes and rerun the candidate checks.
3. Publish the reviewed commit with `cargo publish --locked` from `rust/`.
4. Tag that commit `rust-v0.1.0`, push the tag, and create release notes from
   `rust/CHANGELOG.md`. Verify that docs.rs builds and the published crate can
   be installed with `cargo install --locked defuddle`.

Do not publish the Rust crate through the npm workflow. Keep subsequent Rust
versions and tags independent of the upstream TypeScript version.

Cargo's [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
and [package reference](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
describe the dry run and archive verification steps.
