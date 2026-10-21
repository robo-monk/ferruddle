# F2 response: math evidence versus formula source boundaries

The F2 regression is fixed without adding math conversion. Processing-control
classes no longer produce an unsupported-math warning or protect an entire
prose section as formula source. The warning for actual math remains exactly
`math_not_supported` / `Math support is not yet included.`

## Changes

`math::has_math_markup` now owns broad detection evidence: explicit source,
native/generated math markup, `role=math`, and existing broad class markers.
Classes ending in `_ignore`, `_process`, `-ignore`, or `-process` are excluded
from class-based evidence. This is a per-class rule: an ignore container with
actual descendant MathML still warns.

`math::is_math_source` and the shared `SOURCE_SELECTOR` identify explicit source
attributes, native MathML/math scripts/TeX annotations, and exact known source
roots. Broad class prefixes, a `latex` class alone, and `role=math` alone no
longer define source boundaries. Standardizer protection and cleanup already
use this shared predicate/selector, so they consume the narrowed rules directly.
Known formula roots such as `.katex`, `.MathJax`, their display roots,
`mjx-container`, and the established source wrappers remain supported.

The Markdown worker added the other half of F2: a source candidate with cached
native prose-block descendants renders its children normally. This uses the
renderer’s existing block-containment cache and adds no descendant rescans.
An explicit source attribute on such a prose container still produces the
correct warning. The worker also owns the independent F1/F3/F5 serializer and
block-separation fixes; this worker did not edit Markdown.

## Regression tests and before evidence

New minimal anonymized fixture:
`tests/data/native/math-processing-controls.html`.

The new public regression exercises default extraction, debug extraction,
`standardize:false`, and standalone `html_to_markdown`. It requires normal
`## Model overview`, `**linear**`, and `- First *item*` Markdown syntax, and
rejects an entire retained `<section>` blob. It also checks the rendered heading,
three paragraphs, two list items, emphasis, strong text, and link. No false math
warning is allowed.

A companion regression replaces the processing classes with a genuine
`data-math="x^2"` source attribute. It requires the same prose conversion in all
four modes while retaining exactly one unchanged math warning.

Before runtime changes, both focused tests failed:

- The processing-control fixture emitted one false math warning.
- The genuine source/prose wrapper was emitted as one retained raw `<section>`
  instead of Markdown headings, paragraphs, lists, and emphasis.

The frozen round-one reviewer binary independently reproduced standalone and
debug extraction on the final fixture. Both had `warningCount: 1`,
`rawSection: true`, `markdownHeading: false`, and `markdownList: false`.
Evidence is under `/home/robomonk/.cache/defuddle-port-hardening/`:

- `math-r2-before.log`
- `math-r2-source-before.log`
- `math-r2-md-before.json`
- `math-r2-extract-before.json`
- `math-r2-after.log`

## Stronger existing source contracts

All 18 existing `sourceContract` entries now execute in three modes: default
extraction, debug extraction, and standalone conversion. Extraction checks both
HTML and CommonMark-reparsed Markdown; standalone checks reparsed Markdown.
Every original source signature and multiplicity remains required, including
MathML element order, all five KaTeX data-math sources, repeated formulas, image
source/alt values, original math scripts as code, and the specific authored raw
TeX fragments. Surrounding prose and the exact warning are required in every
mode. No snapshot, fingerprint, contract reason, allowlist, or upstream fixture
was rewritten by this worker.

Private unit tests additionally cover control-class variants, actual math inside
an ignored container, broad-only detection markers versus source boundaries,
and agreement between every known generated root and the shared selector.

## Focused validation

Commands source `/home/robomonk/.cache/defuddle-port-hardening/env.sh`, which
selects the working 1.99.1 toolchain and the current-only Cargo target.

- `cargo test --manifest-path rust/Cargo.toml --lib math::tests -- --nocapture`:
  **3 passed**.
- `cargo test --manifest-path rust/Cargo.toml --lib math_detection_warns_once -- --nocapture`:
  **1 passed**, preserving existing positive detection and Q6 negatives.
- `cargo test --manifest-path rust/Cargo.toml --lib standardize -- --nocapture`:
  **19 passed**.
- `cargo test --manifest-path rust/Cargo.toml --test math_safety -- --nocapture`:
  **8 passed**, including every source contract in all three modes, after the
  Markdown worker’s native-prose guard and structural serializer landed.
  Full output is recorded in `math-r2-after.log`.

No benchmark was run. Final corpus fingerprints and full validation remain with
the coordinating worker.
