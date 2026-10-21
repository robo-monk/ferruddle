# Math source and sanitizer hardening

Math conversion remains excluded. Detection still returns exactly one warning,
`math_not_supported` / `Math support is not yet included.` No upstream HTML,
TypeScript expectation, or native corpus fingerprint was changed by this worker.
The coordinating worker owns reviewed corpus contracts and final full-suite checks.

## Correctness and security changes

- `math.rs` now owns the source predicate, root selector, and source attribute
  names used by structural cleanup and Markdown. Detection recognizes the same
  supported markup and positive delimiter examples. Library detection is lazy;
  tight dollar delimiters and narrow template/regex/drive-path checks suppress
  the five Q6 technical-prose false positives. `is_math_image` lets the pipeline
  detect newly promoted source images without another complete math scan.
- Empty `data-math`, `data-tex`, and `data-latex` holders survive cleanup and
  count as meaningful section content. Their original source is retained as an
  inert attribute; no TeX parser or conversion was added. Explicit sources
  survive hidden-selector cleanup; generated duplicate visual KaTeX content
  remains eligible for removal. MathML annotation encoding is retained.
- `meta` and `link` are unconditionally unsafe, including explicitly selected
  roots and adapter output when standardization and every optional removal pass
  are disabled. The existing sanitizer still strips active attributes and unsafe
  URLs; poisoned source strings remain escaped inert attribute values.
- Hidden detection now precedes descendant math searches. Partial selector
  matching precedes protected-subtree searches. Empty-node cleanup checks child
  kinds before requesting recursive text. Inline whitespace examines the first
  and last text character using bounded traversal rather than collecting each
  entire subtree text. Standardization uses the shared static-name retag helper;
  exact removal uses the coordinating worker's equivalent fixed-list selector.

The coordinating worker migrated direct helper tests into crate-local modules;
those tests now exercise production modules rather than compiling source copies.

## Regression evidence before the fix

The saved baseline is
`/home/robomonk/.cache/defuddle-port-hardening/baseline`; the baseline executable is
`/home/robomonk/.cache/defuddle-port-hardening/defuddle-baseline`.

The initial new black-box test suite was copied into the baseline source and run
as `cargo test --test hardening_math_safety -- --nocapture`. All four tests
failed against unchanged baseline runtime source:

- The new minimal fixture `tests/data/native/math-source-heading.html` produced
  no `Quadratic Formula` heading (actual empty heading text versus the required
  heading); its two `data-math` sources were removed.
- The corpus-specific source/prose test reached `math--katex-data-math` and
  failed because `Quadratic Formula` had disappeared. The test requires all five
  original sources, including all three occurrences of the repeated `x` source.
- A generic article retained both a full `meta http-equiv=refresh` directive and
  a stylesheet `link` with every optional removal and standardization disabled.
- ChatGPT adapter output retained the same full directives when standardization
  was disabled.

The baseline foundation detector test, with the five Q6 negatives appended,
failed on `The plan costs $10 per user_id and $20 per team.` The exact warning
was present although the input contained ordinary currency prose.

A further baseline CLI probe used the final hidden-source regression input,
saved as `math-hidden-baseline.html` in the same cache directory. Its entire
content was:

```html
<article><p>The explanation introduces the preserved source formulas.</p></article>
```

All four explicit source holders and the `Source formulas` heading were lost.
The math warning remained present.

While strengthening the source gate, CommonMark reparsing also exposed a
preexisting Markdown defect: the second arXiv MathML annotation containing
`\\mathrm{head}_{1},...,\\mathrm{head}_{h}` was truncated to the prefix
`\\mathrm{head}`. The Markdown worker fixed literal text serialization with
numeric entities; the reparsed source now matches the original exactly. This
preserves source and does not perform formula conversion.

## Focused validation after the fix

The verified commands use the private working toolchain:

```sh
RUSTC=/tmp/opus-perf/rustup-home/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/rustc
RUSTDOC=/tmp/opus-perf/rustup-home/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/rustdoc
CARGO_TARGET_DIR=/home/robomonk/.cache/defuddle-port-hardening/target
CARGO_BUILD_JOBS=2
CARGO_INCREMENTAL=0
```

With those environment values and that toolchain's `cargo`:

- `cargo test --manifest-path rust/Cargo.toml --test math_safety -- --nocapture`:
  **6 passed**. This includes all 18 executable `sourceContract` entries from
  `tests/data/quality/math.json`, 128 original structured/image/source holders,
  repeated-source multiplicity, specific raw TeX examples, section prose, and
  original versus HTML versus CommonMark-rendered Markdown source signatures.
  MathML signatures also preserve element order. The empty lazy MathJax
  placeholder is explicitly outside the four recoverable formulas in its
  contract. The KaTeX data-math case requires its heading and all five sources.
- `cargo test --manifest-path rust/Cargo.toml --lib math_detection_warns_once -- --nocapture`:
  **1 passed**, covering existing positive detection, currency, templates,
  shell variables, regex, drive paths, and the exact warning without mutation.
- `cargo test --manifest-path rust/Cargo.toml --lib standardize -- --nocapture`:
  **19 passed**, including the boundary-character equivalence oracle, direct
  structural cleanup, and affected existing source/whitespace regressions.
- Before crate-local migration, the foundation suite passed **15 tests** with
  the initial fixes, including the new Q6 negatives and existing sanitizer cases.

An earlier baseline build temporarily polluted the shared Cargo target with
same-package artifacts. This was diagnosed from embedded baseline source paths,
then invalidated before the successful runs above. Further baseline builds use
`target-baseline`; the coordinating worker is using a separate final validation
target. No test failure was accepted or snapshot updated to accommodate it.
