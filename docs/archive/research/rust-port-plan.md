# Rust port: HTML → cleaned HTML and Markdown

Research baseline: Defuddle **0.19.5**, commit **`556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`**, inspected and tested on 2026-10-20. The accompanying [baseline report](rust-port-baseline.json) records the measured results.

This document records the initial research plan. The implemented library is in
[`rust/`](../rust/README.md). The accepted scope excludes math conversion and
requires an explicit warning when math is detected. Quality is checked through
rendered content and reviewed differences, rather than exact HTML bytes. These
decisions supersede the original full-math and strict-output proposals below.

The recommended target is a **native, synchronous, offline Rust library**, taking HTML plus an optional page URL and returning cleaned HTML and optionally Markdown. Preserve Defuddle's algorithm and output conventions. Use the existing repository fixtures as the initial acceptance corpus, supplemented with the relevant inline regression and security tests.

The first deliverable should be the compatibility harness, followed by a DOM feasibility spike. The largest uncertainties are HTML parser differences and the full math conversion dependency, rather than Rust syntax or HTTP integration.

## 1. Scope and compatibility contract

Target the behavior of [`src/node.ts`](../src/node.ts) with linkedom, full math support, and no network. This corresponds closely to the CLI/Worker's DOM environment without requiring either application to be ported.

Include:

- Main-content selection, scoring, clutter removal, standardization, retries, and sanitized fallback output.
- URL resolution with the page URL and `<base href>`; a URL still matters even when HTML is supplied locally.
- Local site-specific extraction, including structured data embedded in HTML and comment/conversation bodies already present in the document.
- Defuddle's Markdown dialect: tables, footnotes, math, callouts, task lists, highlights, raw media/SVG, and its escaping behavior.
- Metadata needed by the algorithm. Title affects heading removal; description affects boilerplate removal; image metadata affects cover-image removal; JSON-LD can determine the content itself. These cannot simply be removed because the public use case only needs body content.
- Options for content selection and cleanup, reply inclusion, and diagnostic traces. Sanitization remains unconditional.

Leave URL fetching, proxy support, API transcript retrieval, browser extension integration, rendered/computed layout, imperative shadow roots, website UI, and frontmatter formatting outside the first release. Preserve declarative shadow roots and React streaming HTML: these are present in static input and require no JavaScript execution.

The offline boundary is experimentally supported: for **all 226 fixtures, on both linkedom and JSDOM**, the eight exported parity fields were identical between the upstream async API with rejecting fetch and `useAsync: false`. This establishes equivalence for this corpus, not every possible page.

A proposed public interface is:

```rust
pub fn extract(
    html: &str,
    page_url: Option<&str>,
    options: &Options,
) -> Result<Extraction, Error>;

pub fn html_to_markdown(clean_html: &str) -> Result<String, Error>;

pub struct Extraction {
    pub html: String,
    pub markdown: Option<String>,
    pub metadata: Metadata,
    pub diagnostics: Option<Diagnostics>,
}
```

Return HTML and Markdown in separate fields. A test adapter maps them to upstream `content` and `contentMarkdown`. Normal malformed HTML should recover as the source does; an ordinary failed selection should proceed through retries and sanitized fallback. Keep fatal resource/serialization errors distinct from recoverable extraction failures. No JavaScript runtime or HTTP client is needed in the production dependency graph.

## 2. What actually needs porting

The repository has **22,733 TypeScript source lines** under `src/`, including 7,772 in extractors. This is a substantial rules engine, with behavior distributed across modules.

| Responsibility | Source | Porting implication |
| --- | --- | --- |
| Orchestration, retries, output safety, URL/image finalization | [`defuddle.ts`](../src/defuddle.ts), 1,926 lines | Preserve the sequence and retry acceptance rules. |
| Content candidates and removal scores | [`removals/scoring.ts`](../src/removals/scoring.ts), 568 lines; [`constants.ts`](../src/constants.ts), 1,184 lines | Selection and removal use different heuristics. Selector priority is part of the algorithm. |
| Structural cleanup | [`standardize.ts`](../src/standardize.ts), 1,833 lines | This is ordered rewriting, with repeated flattening/divider passes. |
| Footnotes, code, images, math, headings, callouts | [`elements/`](../src/elements), 3,845 lines | Dedicated subsystems, not generic tag stripping. |
| Clutter and content patterns | [`removals/`](../src/removals), 2,393 lines | Preserve ancestor protection, code/footnote exceptions, and content-boundary checks. |
| Metadata | [`metadata.ts`](../src/metadata.ts), 771 lines | Preserve candidate precedence and placeholder rejection. |
| Markdown | [`markdown.ts`](../src/markdown.ts), 1,024 lines | Turndown plus 20 custom rules and pre/post-processing. |
| Site adapters | [`extractor-registry.ts`](../src/extractor-registry.ts), [`extractors/`](../src/extractors) | Registry ordering and multiple return paths matter. |

### Extraction sequence

[`parseAsync()`](../src/defuddle.ts#L816) first tries an extractor that prefers async work, then runs the local parser, then tries an async fallback if local extraction has no words. The proposed Rust API uses the local path, with local adapter behavior retained.

[`_parseWithRetries()`](../src/defuddle.ts#L94) normalizes attributes and promotes noscript images before attempting extraction. Each internal attempt then follows this order:

1. Read/cache JSON-LD and metadata before scripts can be removed.
2. Try a site extractor. An extractor can return ready-made HTML **or** a selector that re-enters the normal pipeline with selected removals disabled. Conversation extractors can also run a nested Defuddle pass.
3. Cache small-image information; clone and normalize the document; hoist declarative shadow roots; resolve React `$RC("B:n","S:n")` swaps.
4. Select main content: explicit selector, prioritized entry points plus content scores, table-layout detection, or block scoring. Prefer a meaningful more-specific child under the source's conditions. JSON-LD can refine a body selection.
5. Remove adjacent metadata and `<wbr>`; adopt external footnotes; **standardize footnotes and callouts before removals**.
6. Remove small images, hidden content, eyebrow labels, exact/partial selector matches, low-scoring blocks, and content patterns, in that order.
7. Standardize content: spacing/comments/headings, preformatted code, math → code → headings → images rules, SVG colors, custom elements, spans, layout tables, wrapper flattening, attribute cleanup, special links, empty content, trailing headings/dividers, and empty lines. Preserve the repeated passes and their order.
8. Resolve URLs/srcsets/posters; deduplicate images; remove the metadata cover image; **unconditionally sanitize** the selected root and descendants; serialize the root with outer HTML.

Ready-made extractor HTML takes a shorter path: parse → sanitize → resolve URLs → `standardizeExtractorOutput()` → serialize the container's children. Whole-body and schema fallbacks have their own sanitization paths. Forcing these through one generic standardizer changes output.

### Retry rules are part of extraction quality

Port the actual branches, not a single “be less aggressive” retry:

- Below 200 words: disable partial-selector removal; accept only if the result grows by **more than 2×**.
- If still below 50 words: retry without hidden-element removal, then target the largest hidden subtree when available. The latter has its own word-count/focused-content acceptance conditions.
- If still below 50 words: disable low-score, partial-selector, and content-pattern removal; accept more words.
- If schema text has more than **1.5×** the selected words: find its corresponding DOM element and re-run extraction, or return sanitized schema text.
- Keep a successful earlier result when a later attempt fails; defer the whole-body fallback until extraction attempts are exhausted.

See [`schema-fallback.test.ts`](../tests/schema-fallback.test.ts), [`degraded-fallback.test.ts`](../tests/degraded-fallback.test.ts), and the hidden/listing fixtures. Several short fixtures exercise retries automatically, so a correct initial pass alone is insufficient.

### Browser-dependent behavior

[`utils.ts`](../src/utils.ts), [`hidden.ts`](../src/removals/hidden.ts), and [`small-images.ts`](../src/removals/small-images.ts) distinguish real browser measurements from server DOM behavior. The linkedom compatibility layer supplies empty stylesheets and a minimal computed-style stub. Hidden removal primarily uses inline styles and framework classes in the selected target.

Do not add a CSS layout engine for the initial port. Implement the server behavior, including responsive show/hide class exceptions, explicit image dimensions, SVG viewBox dimensions, and CDN/srcset size hints. Browser parity can be a later wrapper with separately supplied observations.

## 3. Rust implementation choices and early proof obligations

**DOM recommendation: start with `dom_query` 0.28.0 behind a small private adapter.** Its documented API supports HTML documents/fragments, mutation, traversal, cloning, and selectors including `:has()`. Its published source uses html5ever, an arena tree, and a selector parser with `:has()` enabled. This fits the repeated DOM rewrites. [API documentation](https://docs.rs/dom_query/0.28.0/dom_query/), [published source](https://docs.rs/crate/dom_query/0.28.0/source/src/document.rs).

Disable its default Markdown feature: its built-in serializer is not Defuddle's Markdown contract. Keep DOM/node handles private; copy selected node IDs into a vector before mutations. Make every retry clone an unchanged working baseline rather than the previous attempt's damaged tree. Use a per-extraction context for metadata, caches, diagnostics, and flags; do not reproduce the TypeScript module-level `_debug` state as a global.

The DOM adapter should provide raw textContent, parent/child/sibling traversal, containment/document order, selection/matching/closest, clone/move/unwrap/rename, safe text/attribute setters, template-content access, and separate inner/outer serialization. Prefer stable node IDs for internal retry targets; keep generated CSS selectors for public diagnostics and `contentSelector` compatibility.

Before committing to the dependency, run these probes on real fixture fragments and representative inline cases:

- Every constant/rule selector compiles; `:has`, nested `:not`, case-insensitive attributes, sibling combinators, quoted IDs/classes, and `:scope > ...` give the expected matches. Source inspection does **not** establish element-scoped `:scope` behavior: the inspected matcher creates a default matching context. Implement direct-child queries or scoped matching in the adapter if needed.
- Selection excludes the context element, returns document order, and remains predictable after detach/move operations. Preserve the source's stable tie ordering when sorting candidates.
- Templates have separate inert content; nested declarative roots can be hoisted while ordinary templates are removed. Noscript images remain accessible without enabling JavaScript execution.
- Table fragment context, implicit `<tbody>`, malformed nesting, SVG/MathML namespaces, adjacent text normalization, and serialization are understood. Use context-aware fragment parsing where a body-context fragment would discard a `<tr>` or `<td>`.
- Clones are independent, including templates; raw text extraction does not insert whitespace or silently omit text that upstream counts.

If `dom_query` fails a required probe, first implement a narrow adapter workaround. The fallback is **html5ever + a controlled mutable tree + selectors**, accepting the extra adapter work. `scraper` exposes an HTML5 tree/query API, but does not remove the need for this mutation layer; `lol_html` is designed around streaming rewriting, whereas this algorithm needs whole-tree scoring and retries. These are design assessments, not measured Rust benchmarks. [scraper documentation](https://docs.rs/scraper/latest/scraper/), [lol_html documentation](https://docs.rs/lol_html/latest/lol_html/).

**Markdown recommendation: port Defuddle's rule layer explicitly.** Use a bounded `htmd` spike to decide whether its Turndown-inspired core and custom handlers can supply the base conversion. Require the cleaned-HTML → Markdown corpus to pass without changing expectations before adopting it. Otherwise implement the base traversal/whitespace rules and the 20 overrides directly on the selected DOM. `htmd` documents custom handlers and Turndown compatibility, but that does not demonstrate compatibility with Defuddle's overrides. [htmd source and API examples](https://github.com/letmutex/htmd).

The dialect needs ATX headings, `---`, `-` bullets, tab-indented nested lists, ordered starts, checked tasks, dynamic code fences, figure/caption logic, best-image URLs, Obsidian media embeds, `==highlight==`, strikethrough, citations, footnote definitions, and foldable callouts. Simple/ragged tables become pipe tables; rowspan/colspan tables retain cleaned HTML; layout tables flatten. Keep safe raw media/SVG/sup/sub output where upstream does. Reproduce rule precedence and nested conversion behavior.

**Math is a separate compatibility project.** The test config aliases math to `math.full.ts`; the Node build also uses full math. Upstream combines `temml` for LaTeX → MathML and `mathml-to-latex` for the opposite direction. Preserving only `data-latex` can pass some Markdown examples while failing cleaned HTML or MathML-only equations.

Start by preserving existing MathML and recovering original LaTeX from annotations/attributes/scripts. Port MathJax/KaTeX detection and reconstruction, equation-table handling, block/inline decisions, and the trust hierarchy for degraded `data-latex`. Isolate both conversion directions behind Rust traits. Evaluate a native renderer against the exact math subset before selecting it; for example, `latex2mathml` documents unsupported commands and cannot be assumed to replace Temml. Implement/extend the necessary conversions if native candidates fail. Full HTML/Markdown parity stays an explicit release gate; a LaTeX-preserving intermediate release must identify its math limitation. [latex2mathml support matrix](https://docs.rs/latex2mathml/latest/latex2mathml/).

Other likely dependencies are `url`, `serde`/`serde_json`, and `regex`. Use insertion-preserving JSON maps where source traversal order selects metadata, and explicitly reproduce numeric ordering where JavaScript objects depend on it. Rust `regex` does not support lookaround/backreferences; rewrite those checks with explicit boundaries/scans rather than copying patterns blindly. Existing examples include partial-selector exceptions for `main-access-wall`, `logo`, and `h1-related`. [regex syntax constraints](https://docs.rs/regex/latest/regex/), [URL parsing/joining](https://docs.rs/url/latest/url/).

Also audit JavaScript UTF-16 string lengths versus Rust byte lengths, ASCII `\w`/word boundaries versus Unicode defaults, NBSP/zero-width characters, CJK word counting, and one-time fragment decoding. `formatFootnoteId()` uses JavaScript character codes in a custom percent representation; ordinary UTF-8 URL encoding is not an equivalent replacement. Preserve floating-point scores and strict threshold comparisons.

Define the input boundary explicitly: Rust `&str` is valid UTF-8. Two `x-article-surrogates.test.ts` cases construct JavaScript strings with lone UTF-16 surrogates on opposite sides of markup; those inputs cannot be represented directly as Rust strings. Keep intact-emoji behavior in scope, and classify the invalid-UTF-16 repair cases as JS-input compatibility work unless a separate input representation is deliberately added. Do not invent emoji by joining replacement characters after decoding invalid bytes.

No Cargo toolchain was available in the research environment. The crate assessments above are documentation/source audits; compilation, selector probes, MSRV, and performance remain milestone work.

## 4. Existing tests and what they prove

Measured inventory:

- **226** top-level HTML fixtures, each paired with an expected Markdown file.
- **4** explicit HTML goldens: numeric-anchor footnotes, sidebar-toggle checkbox, equation-table MathML, and small SVG icons.
- **1** additional nested YouTube HTML fixture, loaded by transcript-specific tests; the general fixture discovery is nonrecursive.
- **43** test files in `tests/`, plus **2** under `website/tests/`. The complete run executes **826** tests.
- Existing inputs combine small regressions and larger saved website structures, including Wikipedia, GitHub, Substack, LessWrong, arXiv-style math, news/blog pages, and conversations. They are useful real-page regressions, not a comprehensive live-web benchmark.

Exactly 100 fixtures specify a URL in the JSON comment convention; 126 use the filename-derived fallback, which can be synthetic. Preserve those exact URLs when reproducing the existing tests. Correct-origin routing tests can be added separately with independently reviewed expectations; silently substituting a website URL can select a different extractor and change the baseline.

The [test scope matrix](rust-port-test-matrix.json) enumerates every one of the 45 discovered test files, its observed test count, proposed phase, and whether to port, adapt, split, or exclude its behavior. Mixed-file counts are not claims that every assertion is in scope.

Feature groups include 28 `footnotes--` fixtures, 16 code-block fixtures across two naming prefixes, 14 `math--` fixtures, 19 `elements--` fixtures, 15 `content-patterns--` fixtures, 10 `metadata--` fixtures, 4 table-layout fixtures, 31 `general--` fixtures, and 55 `issues--` fixtures. Categories overlap conceptually; report actual fixture IDs as well as group totals.

Observed linkedom extraction paths:

| Path | Cases |
| --- | ---: |
| Generic pipeline | 209 |
| Hacker News / X article | 3 each |
| ChatGPT / Substack | 2 each |
| Mastodon, Reddit, BBCode data, GitHub, Gmail, LeetCode, Twitter | 1 each |

That is **17 specialized cases across 11 extractor types**. The registry has 26 entries, so passing 226 fixtures does not establish coverage of every local adapter. A GitHub-looking filename may still use generic extraction; use the recorded `extractorType`, not the hostname, to classify coverage.

The main fixture test compares a JSON code-fence preamble with **title, author, site, published**, followed by Markdown. It only asserts cleaned HTML for the four HTML goldens. It excludes timing, debug information, most metadata, and word count. It also auto-creates missing expected files: the new porting exporter deliberately fails when an expectation is missing instead.

Actual baseline verification:

```bash
npm ci --no-audit --no-fund
npm ci --prefix website --no-audit --no-fund
npm test -- --maxWorkers=2
npm run test:jsdom -- --maxWorkers=2
npm run build
```

Both backends pass **826/826** tests; the build passes with existing webpack size warnings. Runs used Node **26.8.2**, outside the repository's CI Node 20/22/24 matrix. Initial simultaneous unrestricted runs exceeded some CLI subprocess tests' five-second limit; those tests pass in isolation and both complete suites pass with bounded workers. No production Worker calls were needed.

### HTML parity needs an explicit policy

Comparing the two exported backends yields 100 raw case differences, mostly attribute order. Using the existing normalization leaves **five** differences: cleaned HTML for `elements--data-table`, `extractor--chatgpt-citations`, `general--daringfireball.net-2025-02-the_iphone_16e`, and `table-layout--paulgraham.com-makersschedule`; word count for `security--math-literal-text`. All 226 Markdown bodies and four golden metadata fields agree after the upstream comparison rules.

Examples include implicit `<tbody>` and repaired nested anchors. A native HTML5 parser can reasonably resemble JSDOM here. Therefore:

1. Keep linkedom as the default exact target, including normalized oracle HTML, and retain the separate JSDOM export as evidence.
2. Require the existing Markdown/metadata goldens and four explicit HTML goldens unconditionally.
3. During the DOM spike, decide whether each parser-only HTML difference warrants a compatibility shim or a narrowly documented deviation. Any deviation must name fixture IDs/fields and preserve content, links, math, attributes, and security. Report strict parity and approved deviations separately; never call a waived result an exact pass.
4. Do not broadly strip `<tbody>`, whitespace, attributes, or wrappers in the comparator to make a score improve. Code whitespace, entities, Unicode, footnote targets, captions, and table structure are substantive output.

## 5. Executable verification bridge added with this plan

[`scripts/porting/`](../scripts/porting) now contains an exporter and a comparator. They run through Vitest with the existing full-math configuration and directly reuse [`tests/helpers.ts`](../tests/helpers.ts) normalizers. They do not run during normal test discovery or change production code/expectations.

Generate a portable reference bundle from the repository root:

```bash
port_oracle_parent="$(mktemp -d)"
TZ=UTC DEFUDDLE_ORACLE_DIR="$port_oracle_parent/linkedom" \
  npx vitest run --config scripts/porting/vitest.config.ts

TZ=UTC DOM=jsdom DEFUDDLE_ORACLE_DIR="$port_oracle_parent/jsdom" \
  npx vitest run --config scripts/porting/vitest.config.ts
```

Each bundle has a manifest and 226 JSON cases containing the **actual input HTML**, the exact fixture URL, hashes, and reference HTML/Markdown/metadata. Generation first checks every existing Markdown golden and all four HTML goldens; it writes nothing until validation succeeds. It rejects an existing output directory. Fetch is rejected at both the injected and global entry points. Each case also records any synchronous-mode difference.

The manifest records the upstream revision/version, DOM/Node versions, UTC timezone, options, source/lockfile hashes, and individual case hashes. Input and original expected-file hashes identify the exact corpus. Pin/copy this bundle into the Rust project with the upstream license and attribution so ordinary Rust tests can run without Node or network.

The future Rust parity runner should read every manifest case and emit one JSONL record:

```json
{"id":"fixture-name","result":{"content":"<article>...</article>","contentMarkdown":"...","title":"...","author":"...","site":"...","published":"...","wordCount":12,"extractorType":null}}
```

Then the **existing comparator is executable now**:

```bash
TZ=UTC \
DEFUDDLE_ORACLE_DIR="$port_oracle_parent/linkedom" \
DEFUDDLE_RUST_RESULTS=/tmp/defuddle-rust-results.jsonl \
  npx vitest run --config scripts/porting/vitest.config.ts
```

The comparator requires the complete unique ID set, verifies case/input hashes, and compares all eight parity fields. HTML is parsed with linkedom and attributes sorted without discarding values. Only trailing Markdown whitespace and embedded SVG attribute order outside fenced code are normalized. Leading body whitespace remains significant, matching the upstream comparison of metadata preamble plus Markdown. Other formatting/content remains significant. No HTML/Markdown substring or word-count similarity is accepted as parity.

The bridge has been checked against both generated 226-case bundles. A reference-output self-check passes; deliberate missing cases, duplicate cases, HTML attribute changes, and Markdown changes are rejected. This validates the bridge, **not a Rust implementation**. A standalone strict TypeScript check also passes.

Expand the bridge during implementation with portable records for inline tests: HTML/URL/options plus exact output or explicit assertions. Tests that mock internal functions or mutate a live DOM need behavior-equivalent Rust tests, rather than a mechanical translation of the JavaScript harness.

## 6. Implementation milestones and acceptance gates

Every milestone should name a fixed set of fixture/test IDs in a coverage manifest and pass **100% of that declared set**. All other failures remain visible as pending work. A rising percentage on an undocumented changing subset is not an acceptance gate.

### M0 — Pin the specification and native harness

Vendor the validated linkedom bundle and retain the JSDOM comparison. Create the Rust crate, output adapter, per-fixture `cargo test` runner, JSONL parity example, coverage manifest, and CI. Keep input/golden hashes and attribution intact. Split the comparisons into extraction HTML and Markdown-only tests: feed **reference cleaned HTML** directly to the Rust Markdown converter to isolate conversion failures from extraction failures.

Keep the initial crate in `rust/` in this checkout so source and parity changes can be reviewed together. A proposed layout is:

```text
rust/
  Cargo.toml, Cargo.lock
  src/lib.rs, options.rs, result.rs
  src/dom.rs, pipeline.rs, metadata.rs, selectors.rs
  src/removals/, standardize/, extractors/, markdown/
  tests/corpus.rs, regressions.rs, security.rs
  tests/data/upstream/manifest.json + 226 case JSON files
  examples/parity.rs
```

Once implemented, the Rust CI gates should include:

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo clippy --locked --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path rust/Cargo.toml
cargo run --locked --release --manifest-path rust/Cargo.toml \
  --example parity -- rust/tests/data/upstream > /tmp/defuddle-rust-results.jsonl
```

These are future commands, not commands that succeed in the current research-only checkout. Run the existing bridge comparator against that JSONL in a separate compatibility job; keep the normal native test job independent of Node and HTTP at test runtime. Preserve the frozen bundle if the crate later moves to its own repository.

**Gate:** Rust can enumerate exactly 226 cases; the harness fails for missing/duplicate cases and deliberate content/attribute changes. Markdown-only and full-extraction failures are separately reported. Establish the Node/Rust toolchain versions and a clean-checkout reproduction command. Extend portable records to the security/selector probes needed for M1.

### M1 — Prove the DOM and safety boundaries

Implement the private DOM adapter and run the probes from section 3. Implement escaping, dangerous-URL checks, root/descendant sanitization, inert templates, and safe fallback serialization before complex heuristics. Settle the parser-difference policy using the measured backend cases.

**Gate:** all constant/rule selectors compile or have explicit tested adapter equivalents. Static shadow/streaming and noscript probes retain content. Ported tests from `url-scheme-sanitization`, `template-sanitization`, `svg-sanitization`, and unsafe-root/fallback cases pass, including cleanup disabled. Clone independence and document-order tests pass. No blanket comparator exceptions are introduced.

### M2 — Markdown converter in isolation

Time-box the `htmd` integration spike; retain it only if it supports the required rules and precedence. Otherwise implement the renderer on the chosen DOM. Port the custom table/list/figure/link/code/footnote/callout behavior and post-processing. Math conversion remains a separately reported dependency until M4.

**Gate:** all declared non-math Markdown-only corpus cases pass exactly, along with the applicable assertions from `markdown.test.ts`, `markdown-xss.test.ts`, `inline-tight-spacing.test.ts`, and `heading-serialization.test.ts`. Include code-fence injection, entities, unusual link destinations, nested footnotes/callouts, ragged tables, and raw HTML retention. Math-dependent failures remain named and visible.

### M3 — Generic extraction and retries

Port constants, metadata precedence/JSON-LD, content boundaries, main-content selection, both scoring mechanisms, hidden/small-image/selector/content-pattern removals, and every retry/fallback branch. Implement a first end-to-end vertical slice using ordinary prose, then add layout/listing/hidden/schema cases. Retain step/removal diagnostics for differential debugging.

**Gate:** exact outputs for the declared generic prose/layout/metadata/hidden/listing subset; focused tests show each retry threshold and later-attempt failure behavior. Relative URLs, `<base>`, fragment links, CJK, inline spacing, metadata cover-image decisions, and `contentSelector` handling match. Source goldens remain unchanged.

### M4 — Rich standardization and full math

Complete footnotes, code highlighters/line numbers, lazy images/srcset/picture/captions, math, callouts, SVG styling, table handling, custom elements, and repeated structural cleanup. Preserve cross-subsystem ordering. Prove both math conversion directions against the reference HTML, not just rendered appearance.

**Gate:** **209/209 generic cases** pass the full HTML/Markdown parity contract, subject only to explicitly reported parser deviations; all four existing HTML goldens pass. Port applicable `math-variants`, `lazy-images`, `media-removal`, schema, and security assertions. Unsupported math becomes visible work, never a silent fallback counted as full parity.

### M5 — Offline site adapters and replies

Port local registry behavior in source priority order. Cover the 11 observed extractor types first, then local branches covered by relevant inline extractor tests: conversations, Reddit author/DOM variants, Twitter/linkedom, representable X article inputs, attributes/options, and locally present video metadata/transcripts. Reddit feed fetching remains outside scope. Preserve direct-HTML versus selector-returning adapters and nested conversation cleanup. Preserve `includeReplies` semantics.

**Gate:** **226/226 original fixtures** pass Markdown and metadata; every case also passes oracle HTML/wordCount/extractorType comparison or has an individually recorded parser deviation. All applicable poisoned-attribute extractor tests pass. The adapter matrix identifies local adapters without fixture coverage and network-only branches outside scope; no “all 26 covered” claim is inferred from the corpus.

### M6 — Freeze compatibility and prepare a release

Finish the inline regression matrix, option combinations, error/fallback behavior, and security rendering tests. Add bounded fuzzing for malformed/deep HTML and sanitizer round trips. Run the frozen corpus on the supported native targets; add WebAssembly only if desired as a separate build gate. Review all deviations and dependency/license notices.

**Gate:** zero unclassified in-scope failures, zero security exceptions, no automatic golden regeneration, reproducible clean-checkout `cargo test --locked`, and a complete strict-parity/deviation report. Benchmark HTML parsing + extraction + Markdown using manifest URLs and identical options. Report throughput, p50/p95 latency, and peak memory; separate process startup from warm parsing. The existing `perf.test.ts` excludes Markdown and derives URLs differently, so it is not the end-to-end port benchmark.

## 7. Security and test ownership throughout

Apply the source's security requirements on every output route, including retry winners, schema text, extractor HTML, whole-body fallback, and unsafe selected roots. Remove forbidden active elements/templates, event handlers, iframe `srcdoc`, and dangerous URI attributes. Match the whitespace/control-character normalization in `isDangerousUrl()`, the inline-image exception, and the stricter iframe rule. Preserve safe embeds/math/SVG rather than discarding those features to pass a safety check.

Build HTML with text nodes and attribute setters; when a string builder is necessary, escape every page-derived value at interpolation. A later sanitizer does not replace that guarantee. Test raw adapter builders with poisoned attributes, following `extractor-xss.test.ts`. For Markdown, render with the repository's `marked` harness and inspect the resulting DOM, following `markdown-xss.test.ts`; native rendering tests can supplement this. Check repeated parse/serialize cycles for template/namespace mutation attacks.

Maintain a file-by-file test matrix: port local extraction/Markdown/security behavior; translate DOM-specific mocks into equivalent Rust failure tests; exclude remote fetch/proxy, website UI, JS bundle exports, and browser-live-DOM integration with a written reason. The all-upstream 826 count is a baseline for this repository, not the number of Rust tests that must mechanically exist.

For a newly discovered extraction bug, add a minimal anonymized input and independently correct expected result, show that it fails before the fix, and then passes. Keep these distinct from the pinned compatibility corpus. Upgrade upstream via a separately reviewed oracle regeneration and fixture diff. Never change expectations to conceal a port failure.

## 8. Recommended first implementation slice

Start with **M0 and M1**, then implement one complete prose fixture through selection → cleanup → safe HTML → Markdown. Use an ordinary article plus `elements--data-table`, `issues--react-streaming-ssr-colon-id`, `footnotes--numeric-anchor-id`, and `math--equation-table-mathml` as early contrasting probes. The latter cases deliberately expose parser and rich-content risks before the whole heuristic engine is translated.

The source size and dependency gaps make this several weeks of engineering and validation. A provisional planning allowance is **6–10 engineer-weeks** for strong offline parity, with DOM/math spike results determining the revised estimate. This is an estimate, not a measured implementation duration. The phase gates, rather than that estimate, determine when the port is ready.
