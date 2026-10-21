# Release procedure

The crate and CLI are named `ferruddle`. `Cargo.toml` defines the release version
and minimum Rust version. Publication is manual; CI does not upload a crate.

## Validate a candidate

Run the [development checks](development.md) on the intended release commit.
Test the minimum supported version and inspect the package:

```sh
cargo +1.88.0 test --locked
cargo package --locked --list
cargo publish --locked --dry-run
python3 docs/benchmarks/tools/verify.py
```

The package includes library and CLI source, examples, tests, fixtures, README,
changelog, license and lockfile. It excludes `docs/`, benchmark tooling and local
build/reference directories. All 226 upstream HTML cases remain in the package.

Inspect `target/package/ferruddle-<version>.crate`. Run the unpacked tests with a
separate Cargo target, as the [CI workflow](../.github/workflows/ci.yml) does.
For local package checks on an uncommitted tree, Cargo requires `--allow-dirty`.
Use a clean commit for release.

The [benchmark verifier](benchmarks/tools/verify.py) checks raw evidence, source
archives and measured inputs. Its current-source check permits only explicit
crate/CLI naming, layout and documentation differences. If extraction code or
locked dependencies differ, run a new comparison before making performance
claims. Preserve saved measurements and source hashes.

## Publish

Wait for Linux, macOS, Windows and minimum-version CI to pass on the release
commit. Then:

1. Confirm crates.io name availability and ownership. Authenticate with Cargo
   without saving a token in this repository.
2. Set the README dependency and CLI install command for the registry release.
   Date the changelog and validate the reviewed release commit.
3. Run `cargo publish --locked`.
4. Tag the published commit as `v<version>`. Push the tag and create release notes
   from [CHANGELOG.md](../CHANGELOG.md).
5. Verify docs.rs and installation of the published CLI.

CI checks that `v*` tags agree with `Cargo.toml`. Rust crate versions are
independent of the upstream TypeScript version.

Cargo's [publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
and [package reference](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
describe registry and archive checks.
