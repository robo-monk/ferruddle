# Releasing Ferruddle

The crate and CLI are named `ferruddle`, version 0.1.0, under the MIT license.
The repository is `https://github.com/robo-monk/ferruddle`. Registry publication
is manual; CI does not upload a crate.

## Validate the candidate

Run from the repository root on the intended release commit:

```sh
cargo test --locked
cargo +1.88.0 test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
cargo package --locked --list
cargo publish --locked --dry-run
python3 docs/benchmarks/tools/verify.py
```

The package includes library/CLI source, examples, all tests and fixtures, README,
changelog, license and lockfile. It excludes raw benchmarks, review logs,
research archives and local build/reference directories. The 226 real HTML cases
remain in the package so its quality gate can run from an unpacked crate.

Inspect `target/package/ferruddle-0.1.0.crate` and run the unpacked tests before
publication. Local preparation on an uncommitted tree needs `--allow-dirty`;
CI and the release commit should not.

The [benchmark verifier](benchmarks/tools/verify.py) checks preserved raw results
against the archived sources. It also checks that the current extraction code
still matches the measured implementation after the crate/CLI rename,
documentation changes and move to the repository root. Historical source hashes
are never rewritten to describe the new layout.

## Publish

After the release commit passes Linux, macOS, Windows and minimum-version CI:

1. Confirm crates.io name availability and ownership. `ferruddle` was unregistered
   during preparation; availability is not a reservation. Authenticate with Cargo
   without saving a token in this repository.
2. Replace the README's temporary Git dependency with `ferruddle = "0.1"`, update
   the CLI installation command to `cargo install --locked ferruddle`, date the
   changelog and rerun the candidate checks on the reviewed commit.
3. Run `cargo publish --locked`.
4. Tag that commit `v0.1.0`, push the tag and create release notes from
   `CHANGELOG.md`. Verify docs.rs and the published CLI installation.

CI checks that `v*` tags agree with `Cargo.toml`. Rust crate versions are
independent of the upstream TypeScript release version.

Cargo's [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
and [package reference](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
describe the dry run and archive checks.
