# Pinned TypeScript comparison

Ferruddle is a standalone Rust crate. Its normal builds, tests and CLI need no
Node runtime. The original TypeScript source is preserved only in the checksummed
[measured-source archive](benchmarks/measured-source.json).

The portable [upstream corpus](../tests/data/upstream/manifest.json) contains all
226 original HTML instances, their exact URLs, reference outputs, metadata and
provenance. Rust tests use this bundle directly. Do not regenerate it to accept
a regression; inspect the original input and reviewed semantic contracts first.

## Prepare the benchmark reference

With Python 3.11+, Node.js and npm available, run from the repository root:

```sh
node tools/benchmark/prepare-reference.mjs
node tools/benchmark/benchmark.mjs --validate
node --test tools/benchmark/benchmark-checks.mjs
```

Preparation verifies every archived file before reconstructing the original
reference project under `.benchmarks/reference/`, then installs its locked npm
dependencies with dependency lifecycle scripts disabled. This ignored directory
is disposable and is excluded from the Rust package. Existing nonempty targets
are rejected; preparation does not overwrite a reference installation.

An optional output argument selects another directory. Set
`FERRUDDLE_REFERENCE_DIR` to that directory when running the benchmark. The
coordinator checks the reference files against their original hashes, invokes
the archived Vitest worker there, and passes it the same input selection used by
the current Rust worker. It records reference and Ferruddle source hashes
separately. TypeScript comparison code is not maintained as application source
in this repository.

Follow the [benchmark method](benchmarks/methodology.md) for timing boundaries,
warmups and host monitoring. Run measurements in a new output directory and
never update saved results or hashes in place.

## Updating the reference deliberately

The baseline is Defuddle 0.19.5 at revision
`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`. Both original 826-test DOM-backend
suites passed before export. The source archive retains the original exporter,
comparator and benchmark worker for audit; the
[research record](archive/research/rust-port-baseline.json) records those checks.

To adopt another upstream version, use a separate checkout of upstream Defuddle,
run its original suites and export a new bundle there. Review changed HTML,
metadata, Markdown structure, source preservation and sanitization before
updating the pinned bundle. Preserve the old source and measurement evidence.
