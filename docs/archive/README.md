# Historical records

These records explain the port's design decisions and independent reviews.
They are retained for audit and are not current user instructions.

- [Initial repository research and port plan](research/rust-port-plan.md)
- [Original TypeScript baseline](research/rust-port-baseline.json)
- [Source-test scope matrix](research/rust-port-test-matrix.json)
- [Initial architecture review](reviews/opus-architecture.md)
- [Initial quality review](reviews/opus-quality.md)
- [Initial performance review](reviews/opus-performance.md)
- [Review improvement plan](reviews/IMPROVEMENT_PLAN.md)
- [Final implementation checklist](reviews/hardening-final-checklist.md)
- [Final Markdown peer review](reviews/hardening-markdown-peer-r3.md)
- [Hidden-content recovery peer review](reviews/hardening-hidden-r3-peer.md)
- [Interrupted Opus correctness review](reviews/hardening-opus-correctness-r2-status.md)
- [Original implementation verification](implementation-verification.json)
- [Previous crate release preparation](pre-ferruddle-verification.json)
- [Previous package validation](pre-ferruddle-validation/manifest.json)
- [Original validation logs](reviews/validation/manifest.json)

Historical reports retain their original names, paths, timestamps and wording.
Some refer to the previous workspace layout or local caches that are not part of
a public checkout. Those are historical references, not dependencies of the
library, current tests or benchmark verification. Current entry points are the
[documentation index](../README.md) and [final benchmark report](../benchmarks/README.md).

The original benchmark source files are preserved in a checksummed archive;
the [evidence manifest](../benchmarks/evidence.json) maps the former result
directories to their current locations. Initial and experimental timings remain
historical context and are not combined with the final comparison.
