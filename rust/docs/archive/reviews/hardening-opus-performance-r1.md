# Hardening performance and benchmark review, round 1 (Opus 5.5, xhigh)

Scope: algorithmic equivalence and cost of the implemented performance changes,
new costs introduced by hardening, and validity of the new paired benchmark
harness. Deep correctness/security is covered by
[hardening-opus-correctness-r1.md](hardening-opus-correctness-r1.md); its
optional observation O2 asked this review to quantify two costs, done in
section 1.

No runtime, harness, test, contract, Cargo, or upstream file was modified. No
timer, profile, or full suite was run. Evidence comes from source reading,
dom_query 0.28.0 internals, pure Node checks of the harness functions, and one
**operation-count** probe (no timing) in an isolated copy (see Reproduction).

## Snapshot reviewed

The runtime changed during this review. `math.rs` (00:23) and `markdown.rs`
(00:24) no longer match `frozenSourceHashes` in `hardening-run.json`; all other
26 `rust/src` files match. The probe was re-run on the current `math.rs` and
`markdown.rs`. The harness also changed during the review: `benchmark.test.mjs`
became `benchmark-checks.mjs`, and `nativeExecutable` was added to the summary.
Final hashes reviewed:

| File | SHA-256 (prefix) |
| --- | --- |
| `scripts/porting/benchmark.mjs` | `89bc4311` |
| `scripts/porting/benchmark.shared.mjs` | `63e32709` |
| `scripts/porting/benchmark-checks.mjs` | `fbeb520c` |
| `scripts/porting/benchmark.oracle.ts` | `eed7b13e` |
| `rust/examples/benchmark.rs` | `4354908a` |
| `rust/reviews/hardening-benchmark-harness.md` | `5356cda2` |
| `rust/src/math.rs` / `markdown.rs` (current, post-freeze) | `29432b8d` / `d0509288` |
| `rust/src/standardize.rs` / `pipeline.rs` / `dom/selectors.rs` / `utils.rs` / `patterns.rs` | frozen hashes |

**Root action:** refreeze and regenerate source hashes before the final run. A
benchmark of the current tree is not a benchmark of the frozen hashes.

## Verdict

- **No correctness blocker in the implemented performance changes.** P1, P2,
  P4, P6, P7, the selector index, `boundary_text_char`, and prepared retries are
  exact, or are the intended JavaScript-aligned semantic change (P2). Proofs are
  in section 3.
- **The hardening adds a measurable-by-count cost to `standardize::protected()`**
  (finding 1.1). This is the largest new cost found. It has small, exact fixes.
  Apply them before the final timing, or report the cost as a known regression
  against the prototype.
- **The harness timer placement is fair**, and its input/math-exclusion
  provenance is strong. Before any results are published, three reporting and
  validity gaps must be closed (findings 2.1–2.3). None requires discarding raw
  data, which the harness keeps.
- **The prototype's 48–49% gain does not transfer** to the hardened runtime
  (section 4). Only the paired native repeat with the identical worker can state
  a hardening speedup.

---

## 1. Runtime: costs introduced by hardening

### 1.1 High (fix before final timing): `protected()` does 2× the walks and 5× the selector work

**Where:** `rust/src/standardize.rs:39-42`.

```rust
fn protected(node: NodeRef<'_>) -> bool {
    closest(node, "pre,code,svg").is_some()
        || closest(node, math::SOURCE_SELECTOR).is_some_and(math::is_math_source)
}
```

The baseline did one `closest` with six simple selectors
(`pre,code,svg,math,mjx-container,.katex`). The new version does two complete
ancestor walks for every unprotected node. The second walk evaluates the
current `SOURCE_SELECTOR`, which is 630 bytes and has 28 alternatives, 18 of
them case-insensitive `[class~='…' i]` scans. It runs at about ten call sites:
per text node in three passes, and per element in `remove_empty`,
`whitespace`, `clean_breaks`, `preserve_wrapper`, and span unwrapping.

Two further per-call costs:

- dom_query constructs a fresh `SelectorCaches` in every `is_match`
  (`matcher.rs:40`).
- `dom::matcher()` SipHashes the whole selector string for every cache lookup.
  The per-call key length grew from 38 bytes to 642 bytes.

**Operation counts**, from one HTML pass over all 208 non-math cases on the
current source (no timing):

| Quantity per corpus pass | Baseline form | Current | Ratio |
| --- | ---: | ---: | ---: |
| `protected()` calls | 59,526 | 59,526 | 1.0× |
| Elements visited by `closest` | 357,336 | 701,056 | 2.0× |
| Selector-alternative evaluations (upper bound) | 2.14 M | 10.70 M | 5.0× |
| LessWrong alone (upper bound) | 0.37 M | 1.90 M | 5.2× |

The upper bound assumes each visited element evaluates every alternative. That
is exact for unprotected nodes, which never match. In this non-math corpus, the
old and new walks visit identical elements: no text node sits under
math/MathJax/KaTeX.

The original profile measured `protected()` at about 25–30 ms per baseline pass
(opus-performance.md, H1). The function is now about 2–5× costlier. That
plausibly adds tens of milliseconds per pass, which is several percent to more
than 10% of the prototype's ~430 ms. **This is an estimate, not a
measurement.**

The pages previously flagged as slower (Mastodon, HN, old Reddit, X, ChatGPT,
GitHub PR, Gmail) call `protected()` only 9–84 times each. This cost is
therefore **not** a credible explanation for their earlier ratios, which remain
a noise question (finding 2.3).

**Fix F2 (exact, three local reorderings; avoids 33,804 of 59,526 calls, 57%):**
evaluate the cheap per-node predicate before `protected()`. All predicates are
pure reads at the time they are evaluated.

```rust
// content(): NBSP pass. 16,227 of 16,286 calls avoidable.
for text in root.descendants().into_iter().filter(NodeRef::is_text) {
    let value = text.text();
    if value.contains('\u{a0}') && !protected(text) {
        text.set_text(value.replace('\u{a0}', " "));
    }
}

// remove_empty(): 8,155 of 8,841 calls avoidable.
for node in select(root, "*").into_iter().rev() {
    if !attached(root, node) || ALLOWED_EMPTY_ELEMENTS.contains(&tag(node).as_str()) {
        continue;
    }
    if node.children().iter().all(|child| child.is_text() || child.has_name("br"))
        && node.text().trim().is_empty()
        && !protected(node)
    {
        node.remove_from_parent();
    }
}

// whitespace(), final punctuation loop: 9,422 of 9,450 calls avoidable.
let text = node.text();
let cleaned = BEFORE_PUNCT.replace_all(&text, "$1");
if cleaned != text.as_ref() && !protected(node) {
    node.set_text(cleaned.as_ref());
}
```

The first `whitespace()` text loop could also skip `protected()` when the
transformed value equals the original and is nonempty (6,202 of 13,330 more).
That variant runs `str::replace` and two regexes on protected code text. Adopt
it only if measured. `preserve_wrapper` should test `PRESERVE_ELEMENTS` before
`protected()`.

**Fix F1 (exact, same result, one walk, no per-call key hashing):**

```rust
thread_local! {
    static CODE: Matcher = Matcher::new("pre,code,svg").expect("static selector");
    static SOURCE: Matcher = Matcher::new(math::SOURCE_SELECTOR).expect("static selector");
}
/// Equivalent to `closest("pre,code,svg") || closest(SOURCE).is_some_and(is_math_source)`.
fn protected(node: NodeRef<'_>) -> bool {
    CODE.with(|code| SOURCE.with(|source| {
        let mut nearest_source_seen = false;
        std::iter::once(node).chain(node.ancestors_it(None)).filter(NodeRef::is_element).any(|n| {
            if n.is_match(code) {
                return true;
            }
            if !nearest_source_seen && n.is_match(source) {
                nearest_source_seen = true;
                return math::is_math_source(n);
            }
            false
        })
    }))
}
```

Equivalence: the walk returns true at the first pre/code/svg ancestor
(left disjunct). It also returns true if the *nearest* `SOURCE` match satisfies
`is_math_source` (right disjunct, which uses `closest` semantics). If the
nearest match fails, the walk continues searching only for pre/code/svg. The
"nearest match then predicate" shape is preserved deliberately. Whether a
farther math-source ancestor should also protect is a correctness question for
the math owner.

**Validation:** these are pure reorderings. Require byte-identical corpus
output, debug removals, and readable snapshots before and after. The later H1
idea (one top-down traversal that skips protected subtrees) would make this
O(n). It is not needed for this round.

### 1.2 Medium: `prepare()` always builds the schema root

**Where:** `rust/src/pipeline.rs:269-277`. `schema_element`
(`pipeline.rs:425-458`) clones the full document, sanitizes the entire body,
and walks a pruned text scan. The baseline (and TypeScript
`defuddle.ts:1036-1043`) did this only when main selection fell back to `body`,
or when the schema fallback fired.

The probe shows which corpus pages are affected:

| Case | Input | Main is body | Fallback fired | Eager work needed |
| --- | ---: | :---: | :---: | :---: |
| `general--github.com-issue-56` | 105 KB | no | no | **no (wasted)** |
| `general--substack-app` | 68 KB | no | yes | yes |
| `issues--span-with-block-children-and-schema` | 4 KB | yes | no | yes (once, instead of once per attempt) |

Real news pages with JSON-LD `articleBody` usually have a non-body main root,
so they pay this cost on every extraction. **Fix (exact):** store
`schema_root: OnceCell<Option<OriginalNode>>` in `Prepared`. Fill it in
`prepare` only when `tag(main) == "body"`, and read it through
`get_or_init(...)` in the fallback branch. `prepared.original` is immutable
after `prepare`, so the result is identical whenever it is computed. Treat
`github.com-issue-56` as a per-case regression candidate in the native repeat.

### 1.3 Low / polish

- `math::is_math_source` allocates `to_ascii_lowercase()` for each class token,
  and `markdown.rs:270` now calls it per rendered element. Compare with
  `eq_ignore_ascii_case` instead.
- `removals.rs:91-94` and `standardize.rs:613-615` collect
  `select(node, SOURCE_SELECTOR)` and then call `.any(is_math_source)`. A
  short-circuiting `descendants_it().filter(is_element).any(|n| matches && pred)`
  avoids the vector. These run only on hidden nodes and in `meaningful`.
- Ancestor climbs `patterns.rs:620-625` and `:707-711` still use `words(*p) <= k`.
  `words_at_most(*p, k)` is the same predicate with early exit. These run on
  matched nodes only.
- `metadata.rs:716-726` still compiles two page-derived regexes per document
  (H4, ~1.5% in the original profile). Making compilation fallible fixed the
  panic, but huge site names still pay compile time up to the 10 MiB limit, and
  that time was not measured. A regex-free suffix/prefix comparison would
  remove both costs. Its case-folding semantics must be chosen deliberately.
- For the correctness owner: `schema_element` takes 100 characters *before*
  trimming (`pipeline.rs:428-430`). The baseline and TypeScript
  (`defuddle.ts:714-715`) trim the first paragraph first. Leading whitespace in
  `articleBody` therefore shortens the search phrase.

---

## 2. Benchmark harness

### 2.1 High (before publication): "p95" is the maximum at these sample sizes

`percentile()` (`benchmark.shared.mjs:101-104`) is nearest-rank:
`sorted[ceil(n·0.95) − 1]`. A pure check with the shared functions:

```text
n=7  p95=7  max=7      n=14 p95=14 max=14      n=19 p95=19 max=19      n=20 p95=19 max=20
one 90 ms outlier among 14 samples of 10 ms -> per-case p95 90, max 90; corpus p95 90, max 90
```

Every per-case, per-pass, corpus, and cold `p95Ms` therefore duplicates
`maxMs`. That is the most outlier-sensitive statistic, yet the report labels it
as tail latency. `benchmark-checks.mjs:94` encodes this behavior without naming
it. Only `pageP95Ms`, taken over 208 page medians, is a real p95.

**Fix:** emit `p95Ms` only when `n ≥ 20`. Otherwise report `maxMs`, plus a
spread statistic such as the upper quartile, IQR, or second-largest sample, and
label the basis. Raw samples are retained, so this can be recomputed after the
run. It must be fixed before any document cites per-case "p95".

### 2.2 High (before any hardening-speedup claim): the native repeat is manual and unvalidated

The automated harness compares only current Rust with TypeScript. The
baseline-versus-current procedure (`hardening-benchmark-harness.md:142-191`) is
a set of shell commands. Its outputs never pass through `validateRun` /
`combineRuns`. No executable hash is checked at run time, and nothing computes
the per-case statistics. That procedure is the only valid source for "hardening
made Rust X% faster" or "no case regressed". The historical
`2026-10-20` Rust medians used a different timer method (checksum and result
destruction inside the timer) and cannot serve.

**Fix (small, reuses existing pure functions):** add an optional baseline slot,
for example `DEFUDDLE_BENCH_BASELINE_EXECUTABLE` plus its expected SHA-256.
Schedule it alternately with current Rust using the same pass and mode reversal
(B→C, then C→B). Validate each run with `validateRun`, and require self-stable
outputs. Baseline outputs may legitimately differ from current; list the cases
whose proofs differ. Write `nativeComparison` with per-pass and pooled
per-case median ratios.

Use the existing data as an **A/A noise floor**: compare current Rust pass 1
with pass 2 by per-case medians. That distribution shows how large a
baseline/current ratio must be before it is called a regression. Flag a case
only if it is slower in both passes and beyond that floor. Record the baseline
source hashes (`baseline-source-hashes.json`) and the worker hash in the
summary.

### 2.3 Medium-high (practical): no extraction has run through the new workers

`--validate`, both validation-only workers, and the six pure tests never
execute an extraction. The TypeScript oracle comparison, the Rust
`validate_output` rules, cross-round and cross-pass determinism, and cold/warm
proof equality are therefore unexercised. A failure would surface only in the
quiet window. The probe confirmed that no native snapshot of the 208 cases is
empty or has warnings, and no oracle `content`/`contentMarkdown` is empty. The
abort rules should therefore not fire falsely.

**Fix:** before the quiet run, run one discarded smoke pass, for example
`node scripts/porting/benchmark.mjs /tmp/defuddle-bench-smoke 3 1`, and never
publish its timings.

### 2.4 Medium: executable identity is hashed once and never re-checked

`executableSha256` is computed after `cargo build` (`benchmark.mjs:65`). The
binary lives in the shared `CARGO_TARGET_DIR`, which has a documented history
of artifact contamination. Neither the warm/cold workers nor the manual repeat
re-verify it. **Fix:** copy the built executable, and the baseline executable,
into `OUTPUT/bin/`. Run only the copies, and re-hash each copy in `stable()`.

### 2.5 Medium: provenance gaps

- `tsconfig.json` is not hashed, but Vite passes its options to esbuild:
  `useDefineForClassFields` is derived from `target`
  (`node_modules/vite/dist/node/chunks/config.js:6073-6100`), and the TypeScript
  source uses class fields.
- `node_modules` is represented only by six version strings.
  `node_modules/.package-lock.json`, which records the installed tree
  including linkedom's dependencies, is not hashed.
- Cargo inputs that change the binary are not recorded: `CARGO_PROFILE_*`,
  `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, `RUSTC_WRAPPER`, any
  `.cargo/config*` from `rust/` upward or in `$CARGO_HOME`, any
  `rust-toolchain*` file, and `rustc -vV`. None of these exist today, so this
  hardens provenance only. Record all `CARGO_*` and `RUST*` variables except
  token or secret names.

### 2.6 Medium: label what "TypeScript" means

The TypeScript side runs `src/` through Vitest 5's module runner (Vite SSR
transform with esbuild, tsconfig-driven options, accessor-based module
bindings). It does not run the published `dist/node.js` (tsc ES2020 CommonJS).
The historical harness did the same, so the method is consistent, but ratios
should say "TypeScript source under Vitest/linkedom". Optional: add a
plain-Node worker importing `dist/node.js`, with the same loop and oracle
check, and hash `dist/`.

### 2.7 Low / polish

- **Thermal order:** pass 1 Rust starts immediately after a possibly
  multi-minute release LTO build on a laptop CPU. Pre-build, so the harness
  build is a no-op, or add a configurable cool-down. Record the cpufreq
  governor.
- **GC asymmetry:** Rust frees each DOM inside `extract`, which is timed. V8
  can reclaim a round's DOM garbage during the untimed validation loop, which
  allocates large JSON strings. Document this. Do not force GC.
- **Rotation:** rotating by one page per round keeps every page's neighbors
  fixed. It only changes which page starts the round. Medians are unaffected.
  A recorded seeded permutation would balance position effects if they ever
  matter.
- **Stale claim:** `rust/README.md:157-158` still states the historical
  3.85×/4.42×. Those figures describe the pre-hardening binary under the old
  timer method. Replace them only from the new paired run, and do not mix
  harness generations.

### 2.8 Checked and sound

- **Timer placement.** Both workers time only the parse plus public API call
  per case (`benchmark.rs:214-220`, `benchmark.oracle.ts:78-80`). The round
  timer adds only `Instant`/`performance.now` and result storage. Hashing,
  projection, oracle comparison, and (in Rust) result destruction happen after
  both timers. TypeScript times production `parseLinkedomHTML` plus
  `Defuddle()`/`_parse()` with `useAsync:false`. No dynamic import is on the
  measured path, and no TypeScript branch depends on Vitest's environment.
  Rust parses inside `extract`.
- **Result sinking.** The full Rust result is `black_box`ed, kept until the
  round ends, and then hashed across all eight fields. The compiler cannot
  elide or defer the work.
- **Counts and medians.** There are 2 passes × 7 rounds = 14 samples per page
  and mode. Warmups are kept in `rawRounds`. `validateRun` checks raw versus
  measured agreement, order offsets, dimensions, and identity. The median of an
  even count is the mean of the two middle values. Speedup is the ratio of
  corpus medians, with per-pass values reported as well.
- **Ordering.** Implementation and within-process mode order are fully reversed
  between the two passes. Cold samples alternate both.
- **Inputs and math exclusion.** Both workers independently verify the
  manifest, the `math.json` hash and IDs, the complete ordered 208-case list
  (including LessWrong), every file, HTML, and URL hash, and the identity hash.
  Native rejects any warning, so a non-math case that newly warns aborts the run.
- **Cold method.** Each sample is a fresh process making one call on a 319-byte
  page after imports. It includes Rust lazy regex and selector-index
  construction and V8 lazy compilation. It measures cold API cost, not process
  launch, as documented.

---

## 3. Equivalence of the implemented performance changes

| Change | Location | Result |
| --- | --- | --- |
| Bounded counter `words_at_most` | `utils.rs:35-55` | **Exact.** dom_query `text()` is `once(id).chain(descendant_nodes)` over text nodes (`dom_tree/ops.rs:32-47`), the same preorder as `once(node).chain(descendants_it())`. `WordCounter` state persists across text nodes, so the result equals counting the concatenation. The count never decreases, so stopping at `> limit` is exact. Classification is shared with `count_words`. The unit test covers split words, CJK, tab/newline, NBSP, and non-BMP characters. Optional: extend it to every element of a few corpus documents. |
| Byline ancestor early stop | `patterns.rs:382-389` | **Exact.** Each ancestor's text contains the node's text contiguously, and inner separators are preserved. Distinct word runs stay distinct, and CJK characters each count once, so counts are monotone toward the root. `take_while(≤15)` drops only ancestors whose conjunct was false. |
| `content_start` laziness | `patterns.rs:154-162` | **Exact.** Pure filter; same first candidate and first leaf. |
| Newsletter count-first; boilerplate regex-first; timezone regex-first | `patterns.rs:611-618`, `:696-697`, `:413` | **Exact.** Pure conjunct reordering. `words(n)` is `count_words(&n.text())`, so reusing `text` is identical. |
| `[\s\S]` → `(?s:.)` | `patterns.rs:26` | **Exact.** Both denote every scalar value. |
| ASCII `\b` in `NAV_WORDS` / `CONTENT_DATE` | `selection.rs:99,104` | Intended JS-aligned change. Remaining pre-existing divergences: Unicode `\d` and `(?i)` folding (`ſ`, Kelvin sign) still match where JavaScript non-`u` regexes do not. Low impact (scoring only). |
| `select(..).is_empty()` → `exists` | 25 sites in `patterns.rs`, 2 in `finalize.rs` | **Exact.** Every negation was checked. `exists` has the same root and template exclusion as `select`. |
| Cheap-first `remove_hidden` and partial selectors | `removals.rs:86-107`, `:212-215` | **Exact reordering.** The math-skip predicate itself changed, which is a correctness matter. |
| `boundary_text_char` | `standardize.rs:743-782` | **Exact.** Preorder (and reverse-preorder) walk to the first or last nonempty text node. It is bounded at the root. Templates and comments are excluded as in `text()`. An oracle test covers every node. |
| `tag()` via `qual_name_ref` | `utils.rs:97-101` | **Exact.** `node_name()` is the element local name; this avoids one tendril allocation. |
| Fixed EXACT selector index | `dom/selectors.rs` | **Sound superset prefilter; see below.** |
| Prepared retries and winner-only finish | `pipeline.rs:104-367` | **Preserved:** same thresholds and order, same main and forced-root semantics through clone-stable IDs, sanitize before counting, and exact serialized length in the hidden branch. The only cost regression is 1.2. Edge case: a `body` that is itself the hidden candidate was schema-replaced in the baseline but not now. |

**Selector-index necessity-key proof.** `necessary_key` returns a key only for a
single compound selector: no top-level whitespace (all five ASCII CSS
whitespace characters), no `> + ~ , |`, no escapes, comments, non-ASCII,
unbalanced brackets, or quotes. Every simple selector in a compound is
necessary, so the first one is too.

In dom_query 0.28:
- `has_class` splits the first `class` attribute with `split_ascii_whitespace`
  and compares bytes under NoQuirks, which is hard-coded at `matcher.rs:329`.
- `has_id` compares any `id` attribute exactly.
- Type and attribute names compare the local name, or its lowercase for HTML
  elements.

The index probes every attribute of every namespace. It lowercases tag and
attribute names (a superset) and compares class and id exactly (equal to the
matcher). Each candidate is confirmed by the real matcher, and the output keeps
document order and root exclusion. Selectors without a key go to the `always`
list.

Coverage: 163 of 166 selectors are keyed (50 tag, 30 class, 30 id, 53
attribute). Only 3 are in `always`. The busiest attribute buckets are `href`
(13 selectors), `class` (7), and `role` (7).

Hidden dependency: correctness relies on dom_query ignoring quirks mode. Add a
tripwire to `unusual_css_uses_conservative_fallback`: a document without a
doctype, `class="AD"` and `id="X"`, compared with `.ad` and `#x`. Both paths
match nothing today; the test fails if a dom_query upgrade starts honoring
quirks case-insensitivity.

---

## 4. Actual versus hypothesized gains

| Prototype item (E1–E7) | Hardened runtime |
| --- | --- |
| P1 byline bound and reorderings | Implemented, exact |
| P2 ASCII boundaries | Implemented, with intended semantics |
| P3 `find_main` reuse | Superseded by `Prepared` (selection, base URL, math scan) |
| P4 newsletter regex | Implemented, exact |
| P5 selector index | Implemented, with a stricter key scanner |
| P6 `exists` | Implemented |
| P7 cheap-first | Implemented |

Additional savings not in the prototype:
- Markdown and serialization run only for the winning candidate.
- One math scan instead of one per attempt.
- Bounded boundary-character reads.
- Child-kind check before text in `remove_empty`.

Additional costs not in the prototype:
- `protected()` (1.1).
- Eager sanitized schema search (1.2).
- Math-source checks in `remove_hidden` and `meaningful`.
- Regex-based social-profile matching (neutral by inspection).

Therefore:

1. The measured 48–49% batch reduction and the 0.77 median page ratio belong
   to the prototype on baseline semantics. They cannot be cited for this
   runtime.
2. Only the paired native repeat with the identical worker (2.2) can state a
   hardening speedup or per-case regressions. Only the new paired TypeScript
   run can state a TypeScript ratio. Neither is comparable with the 2026-10-20
   ratios.
3. Expect standardization-heavy pages (LessWrong, Wikipedia, long footnoted or
   tabular articles) to carry the 1.1 cost, and `github.com-issue-56` the 1.2
   cost. Adapter pages are sub-millisecond, so judge them only against the A/A
   floor.

## Priorities for the root

1. Apply F2 (and optionally F1) and the lazy schema root, or record them as
   known regressions. Confirm byte-identical corpus output, debug removals, and
   snapshots. Refreeze the hashes.
2. Fix the p95 labeling (2.1) and add the validated native comparison with the
   A/A floor and executable re-hashing (2.2, 2.4). Small provenance additions
   (2.5) can follow.
3. Run one discarded smoke pass (2.3). Then run the quiet paired run and native
   repeat, and update README and BENCHMARKS only from those results.

## Reproduction (probe; counts only)

The isolated copy is at `/home/robomonk/.cache/defuddle-port-hardening/opus-perf-review/`:
- `probe/`: a copy of `rust/`, with current `math.rs`/`markdown.rs`, an
  instrumented `standardize.rs`/`pipeline.rs`, and the
  `examples/probe_protected.rs` example.
- `scripts/instrument*.py`: the instrumentation edits.
- `scripts/summarize.mjs`: the summary script.
- `protected-html-current.json`: raw per-case counts.

The build used the working 1.99.1 toolchain, `CARGO_TARGET_DIR=…/opus-perf-review/target`,
`CARGO_BUILD_JOBS=2`, LTO off, `--locked --offline`.

```bash
P=/home/robomonk/.cache/defuddle-port-hardening/opus-perf-review
$P/target/release/examples/probe_protected > /tmp/counts.json
SOURCE_SELECTORS=28 node $P/scripts/summarize.mjs /tmp/counts.json
```

Probe counter indices: 0 calls; 1/2/3 elements visited by the pre-code-svg,
`SOURCE`, and baseline walks; 4–11 call/skippable pairs for the NBSP,
`remove_empty`, first whitespace, and final punctuation loops. The schema
counters record schema text present, main is body, schema root found, and
fallback fired. The p95 demonstration used `percentile`/`summarizeMode` from
`benchmark.shared.mjs`. `node --test scripts/porting/benchmark-checks.mjs`
passed 6/6, and `benchmark.mjs --validate` reported 208 cases and 1,372,181
bytes.

## Limits

- No timing of any kind was run, and runtime estimates in 1.1 are labelled as
  estimates.
- Evaluation counts are upper bounds on selector-alternative work, not
  instructions.
- The post-freeze `math.rs`/`markdown.rs` changes were skimmed for performance
  only (linear `retained_html`, per-node `is_math_source`). Their correctness
  belongs to the correctness review.
- TypeScript and native test counts, the MSRV check, and Clippy were not
  re-verified here.
