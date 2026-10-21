# Pipeline, metadata, and adapter hardening

The new anonymous public API fixtures reproduce the source-backed defects from
`opus-architecture.md` D2 and `opus-quality.md` Q1/Q2/Q7/Q11. Upstream TypeScript,
corpus contracts, and corpus hashes were not edited by this work.

## Correctness

- English date parsing now uses ASCII digits and ASCII case folding, fallible
  month/numeric parsing, and validated calendar dates. Arabic-Indic day/year
  digits and `Auguſt` no longer panic or produce malformed publication values;
  ordinary English DMY/MDY dates still work.
- Schema matching searches a sanitized clone, excluding unsafe roots, unsafe
  ancestors, and text contributed by unsafe descendants of eligible wrappers.
  Node IDs map back to the prepared source. A pruned document-order traversal
  computes text/word counts once per visited candidate and keeps first-match
  ties. Paragraph splitting accepts whitespace-only blank lines. When no
  eligible source node matches, the existing sanitized schema HTML fallback
  recovers the full article.
- Social profile recognition now follows the TypeScript ASCII boundary/handle
  rules in `src/removals/scoring.ts:63`. Dropbox/Netflix/Vox and prefix hosts are
  ordinary links; Twitter/X `intent` and Facebook `share` are excluded exactly,
  while longer valid handles remain eligible. All matching handles are checked.
- Positional explicit selection is checked after initial image removal, then
  retained by clone-stable node ID. An explicitly selected unsafe root cannot be
  replaced by schema or retry content.
- Conversation output always removes forms, fieldsets, buttons, select/textarea,
  and input controls independently of outer optional cleanup. Meaningful
  checkbox inputs in list items survive. This corresponds to the nested source
  cleanup in `src/extractors/_conversation.ts:19-27`.
- `NAV_WORDS` and `CONTENT_DATE` use ASCII word boundaries, matching JavaScript
  next to non-ASCII prose. Unit tests cover both positive non-ASCII boundaries
  and negative ASCII word/underscore boundaries.

## Architecture and API invariants

`Prepared` freezes preprocessing/adapter mutations and image removal before
main/schema selection. It carries the resolved base URL, metadata, shared math
warnings, clone-stable selected IDs, and invalid-selector debug information.
`Candidate` owns each cleaned DOM plus sanitized word count, output mode,
metadata, and debug records. Retry thresholds/order remain unchanged. The hidden
retry comparison uses exact trimmed serialized UTF-8 HTML lengths; other losing
candidates are never serialized. Markdown conversion runs once on the winner.

Each attempt still resolves URLs, deduplicates images, and sanitizes before its
word count is used. The direct HTML path retains both sanitizer passes with
standardization and URL resolution between them. Adapter output remains inner
HTML; selected DOM roots remain outer HTML. Math conversion remains excluded,
with exact `math_not_supported` / `Math support is not yet included.` warning.
The original warning scan is preserved. Generic pages have exactly one scan.
Only selector adapters that can materialize source, or a failed/no adapter after
noscript image promotion, can request a second scan when the first had no math;
attempts never scan. Detached adapter/schema HTML is scanned separately when
needed. `dom::preprocess` reports actual image promotion, avoiding duplicate
lazy-context parsing; the source math-image helper remains shared with detection.
Failed adapters are tested not to mutate the shared document. The NYT materialization guard verifies
that TeX embedded only in JSON becomes exactly one warning after insertion.

The adapter's impossible two-Option state is replaced by `Content::{Html,
Selector}`. The adapter entry explicitly accepts `&mut Document`, documents its
mutation contract, and passes parsed URLs to YouTube/Bilibili/Mastodon/Discourse;
YouTube no longer reparses with `unwrap`. `Error` and `WarningCode` are
nonexhaustive. `Options` retains external struct-update construction. Metadata
is documented as untrusted page strings. `finalize::{base_url,urls}` shares URL
resolution with the standalone Markdown path. Existing helpers are used for
short-circuit image/caption existence checks.

The parent agent owns the public module privacy migration and documentation.
The Markdown contract note is that leading H1 removal remains intentional;
relative links and declarative shadow content now follow the separate Markdown
worker's fixes. Do not claim standardized extraction preserves intraword inline
spacing: that optional behavior remains unchanged.

## Regression evidence

Fixtures live in `rust/tests/fixtures/hardening`, with readable schema text in
`rust/tests/expected/hardening/schema-article.txt`. They contain generic names,
example.test URLs, and synthetic story text.

The preserved baseline binary produced:

| Fixture | Baseline | Required result |
| --- | --- | --- |
| schema-body / schema-noscript | empty, zero words | complete 196-word schema article |
| unicode-digit-date | exit 101, numeric unwrap panic | normal return, empty publication date |
| unicode-month-date | exit 101, month unwrap panic | normal return, empty publication date |
| social-link (Dropbox) | source paragraph removed | source paragraph preserved |
| conversation-controls | form and controls retained | controls removed, list checkbox states kept |
| nonascii-nav-boundaries | `éshare漢` navigation retained | removed with ASCII boundaries |
| nonascii-date-boundaries | earlier equal-score article selected | dated article selected |

The public black-box regression suite against preserved baseline source had
**6 failures / 1 passing guard** (adapter-materialized math). The positional
selection probe failed its required root debug identity because a later schema
candidate replaced the explicitly selected unsafe result. Evidence is saved at
`/home/robomonk/.cache/defuddle-port-hardening/pipeline-baseline-regressions.log`.
The individual preserved binary probes also establish both distinct Unicode
panics and the date-selection boundary change.

After fixes, `cargo test --test hardening_pipeline` passed **8/8**, including
wrapped unsafe-schema descendants and promoted noscript formula images with and
without a pre-existing warning. Before the
parent moved private-helper integration tests into crate-local modules, the
focused existing tests passed: extractors **17 / 1 ignored**, metadata **11 / 1
ignored**, pipeline_review **10/10**. Selection unit tests passed **2/2** and the
post-migration architecture tests passed **3/3**, including candidate metrics,
clone-stable forced-root identity, and counters requiring one preparation, one
math scan, and one Markdown render across multiple attempts. The new
unsuccessful-adapter/no-mutation unit test passed **1/1**. Additional
candidate-metric coverage verifies unsafe source cannot inflate retry counts
and uses exact serialized byte length. Full final validation is reported by the
parent.

Tests used the explicit working rustc/rustdoc/cargo 1.99.1 toolchain, two jobs,
and incremental compilation off. A shared-target artifact collision was found
when baseline and working copies built the same package concurrently; all
baseline builds are finished and the parent is using a fresh current-only
output directory for definitive validation. No default toolchain was changed.
