# Architecture review: structure, API, invariants, repeated work

Reviewer: Opus 5.5 (xhigh), independent architecture pass. Date: 2026-10-20.
Baseline: Defuddle 0.19.5 at `556d74e8c8fd68c49912d4b11a4ae9ec04d7b2b5`; Rust crate under `rust/` as of
the latest source mtime (13:10). I changed no runtime source, tests, contracts, Cargo files, or
TypeScript. Diagnostics used only the existing `rust/target/debug/defuddle` CLI. That binary is newer than every file
in `rust/src` (`find src -newer target/debug/defuddle` returns nothing). Inputs were written to
`/tmp/defuddle-arch-review/`. I ran no builds, `cargo test`, or benchmarks.

I read `pipeline.rs`, `finalize.rs`, `dom.rs`, `utils.rs`, `types.rs`, `lib.rs`, `safety.rs`,
`math.rs`, `selection.rs`, `removals.rs`, `standardize.rs`, `markdown.rs`, `footnotes.rs`, and the
`extractors.rs` dispatcher in full. I read the hot-path parts of `patterns.rs`, `metadata.rs`,
`images.rs`, and `code.rs`, plus representative extractors (`comments.rs`, `social.rs`,
`embedded.rs`, `conversations.rs`, `video.rs`). I compared against the TypeScript source only to
check intent (removal scope, `includeReplies`).

## Verdict

Two **proven defects** should be fixed before any structural refactor. Both make public APIs fail on
small, page-controlled inputs:

- **D1 (High):** the Markdown renderer has exponential running time. A 599-byte page took 62 s in
  `extract(..., markdown: true)`, and each additional nesting level roughly doubles the time.
- **D2 (High):** `extract()` panics on page text such as `٣ March 2024` or `3 Auguſt 2024`.

Neither defect is a sanitization bypass. I found no path in the pipeline that skips the
unconditional `safety::sanitize` pass. Both defects are availability failures on untrusted input.
They do not block the structural work, but the fixes are small and local and should land first with
regression tests.

The highest-value structural change is **S1**. Split the pipeline into three stages: prepare once,
attempt per retry, and finish only the winning attempt. Today every retry repeats main-content
selection, math scanning, `<base>` lookup, HTML serialization, and Markdown rendering. All of these
are identical across attempts or discarded with the losing attempts. In the benchmark selection, 146
of the 191 generic cases (no site extractor) have a TypeScript word count below 200, which triggers
at least one retry. Another 41 cases are below 50 and can reach five attempts.

## Prioritized summary

| ID | Priority | Kind | Title | Confidence |
| --- | --- | --- | --- | --- |
| D1 | P0 | Proven defect (availability) | Markdown `children()` renders the probed sibling twice, so time is exponential in inline nesting | High (measured) |
| D2 | P0 | Proven defect (panic) | `metadata::parse_date` unwraps assume ASCII, but the regex is Unicode-aware | High (reproduced) |
| S1 | P1 | Structure + proven redundant work | Prepare once, attempt per retry, finish the winner only | High (by code); speedup unmeasured |
| S2 | P1 | Public API / error design | Hidden-public internals expose `dom_query` types and unsanitized adapter HTML; `Error` variants are never constructed while panics escape | High |
| S3 | P1 | Algorithmic redundancy | Run cheap predicates first and add boundary-character helpers to remove per-node subtree scans | High (by code); speedup unmeasured |
| S4 | P2 | Duplication / invariants | Consolidate `tag`/`retag`/`is`/`contains`/`re!`/`words`; make `dom::retag` take `&'static str` | High |
| S5 | P2 | Type / invariant design | Adapter result should be an enum; adapter mutation of the shared document is implicit | High |
| H1 | P2 | Measured scaling + hypothesis | Superlinear time with nesting depth comes from per-node ancestor and subtree recomputation | Medium (measured curve, cause attributed by reading) |
| H2 | P3 | Hypothesis | Removal passes scan `body` although output is only `root` | Medium |
| O1 | P3 | Optional style | `#[path]` attributes, one-letter helpers, dense one-line statements | — |

---

## D1: Markdown rendering is exponential in inline nesting (proven, P0)

**Where.** `rust/src/markdown.rs:70-110` (`Renderer::children`), specifically line 80:

```rust
} else if !source_block(next) && self.render(next, context).is_empty() {
    children.next();
} else {
    break;
}
```

When an inline-kind child such as `<em>`, `<strong>`, or `<code>` is followed by a different,
non-block sibling, the renderer fully renders that sibling to test whether it is empty. If it is not
empty, the loop breaks and throws away the result. The next iteration of the main loop then renders
the same sibling again. If that sibling contains the same pattern, the cost is
`T(d) = 2·T(d−1)`. The depth guard at `markdown.rs:117` (`depth > 512`) limits recursion depth, but
it does not limit total work.

**Reproduction (extract API via CLI, debug build).** The input nests
`<u><em>w(</em><u>…</u></u>` inside an `<article>`. The `(` prevents `standardize::whitespace` from
inserting a separating space, because it skips pairs that end in opening punctuation
(`standardize.rs:816-826`).

| Nesting depth | Input bytes | HTML only | HTML + Markdown |
| ---: | ---: | ---: | ---: |
| 10 | 383 | 0.093 s | 0.277 s |
| 12 | 437 | 0.100 s | 0.830 s |
| 14 | 491 | 0.105 s | 2.978 s |
| 16 | 545 | 0.113 s | 11.571 s |
| 18 | 599 | 0.123 s | 62.352 s |

HTML-only time grows linearly, and the Markdown time roughly doubles per level. A release build
will be faster by a constant factor, but the growth is still exponential, so a few more levels
offset it.

The same input with a space between `</em>` and `<u>`, or with no adjacent inline-kind element,
stays linear. That isolates the cause to the probe.

The defect is reachable more easily in three cases:

- `html_to_markdown()` (`markdown.rs:21-28`), which never runs standardization.
- `Options { debug: true }`, which skips `whitespace()` (`standardize.rs:89`, `:109-113`).
- `standardize: false`.

In these cases plain `<span><em>a</em><span>…` triggers it. This is inferred from the code, not
measured.

**Related quadratic term.** `source_block()` (`markdown.rs:1015-1058`) runs
`node.descendants_it().any(native)` for every child in every `children()` call. The total is
O(Σ subtree size), which is O(N·depth).

**Suggested change (local, about 30 lines).**

1. In `children()`, collect the children into a `Vec`. Keep one rendered `Option<String>` per index
   and reuse the probe result when the main loop reaches that child. Merged inline runs already
   render through `self.children(sibling)`, and those siblings are never probed, so caching changes
   no output.
2. In `convert_node()` (`markdown.rs:32`), precompute a `HashSet<NodeId>` (or a bitset indexed by
   `NodeId`) of elements that are or contain a native block, using one post-order pass. Store it on
   `Renderer`. `source_block(n)` then becomes a set lookup.

**Validation.**

- Add a test in `tests/markdown.rs` that calls `html_to_markdown` on 40 levels of
  `<span><em>a</em><span>…`. The test should also call `extract(markdown: true)` on the
  punctuation variant above at 40 levels. Before the fix these tests hang until the test harness
  times out; afterward they finish in milliseconds. Avoid wall-clock assertions.
- Run the full corpus gate; the output must be byte-identical.

**Impact.** Denial of service: CPU exhaustion from a few hundred bytes of untrusted HTML through a
public API. Output correctness is unaffected.

---

## D2: page-controlled dates panic in metadata (proven, P0)

**Where.** `rust/src/metadata.rs:684-702` (`parse_date`):

```rust
let month = MONTHS.iter().position(|s| *s == month).unwrap() + 1;   // :698
... day.parse::<u8>().unwrap()                                        // :701
```

The `dmy` and `mdy` regexes use `(?i)` and `\d`. In the Rust `regex` crate both are Unicode-aware:

- `\d` matches any `Nd` digit, such as Arabic-Indic `٣`, and `u8::from_str` rejects it.
- `(?i)` uses simple case folding, so `ſ` (U+017F) matches `s`. `"auguſt".to_lowercase()` is not
  found in `MONTHS`.

`parse_date` runs on the text of up to three siblings after the first `<h1>`
(`metadata.rs:408-432`), so page content controls its input.

**Reproduction.** Both inputs exit with status 101:

```
<article><h1>Title</h1><p>٣ March 2024</p><p>Body …</p></article>
  → panicked at src/metadata.rs:701:31: ParseIntError { kind: InvalidDigit }
<article><h1>Title</h1><p>3 Auguſt 2024</p><p>Body …</p></article>
  → panicked at src/metadata.rs:698:61: called `Option::unwrap()` on a `None` value
```

**Suggested change.** Use `[0-9]` for the day and year. Make the month lookup fallible: compare with
`eq_ignore_ascii_case` and return `String::new()` when no month matches. Replace the unwraps with
`let … else` or `ok()?`. A year matched by `\d{4}` can also contain non-ASCII digits and would flow
into `published` unchanged; `[0-9]{4}` fixes that as well.

**Structural follow-up (see S2).** Adopt a crate-wide no-panic policy for page-controlled data.
Enable `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` and keep a single
allowed `re!` helper for literal regexes (S4).

I checked the remaining runtime `unwrap`/`expect` sites in `markdown.rs`, `math.rs`, `embedded.rs`,
`video.rs`, `footnotes.rs`, `standardize.rs`, and `dom.rs`. They are guarded by regex alternation
structure, prior checks, or ASCII-only domains. `video.rs:10` (`Url::parse(url).unwrap()`) is safe
only because the caller already parsed the same string (`extractors.rs:71`); pass `&Url` instead.

**Validation.** Add one regression test per input in `tests/metadata.rs`. Verify both panic before
the fix.

---

## S1: prepare once, attempt per retry, finish the winner (P1)

**Current shape.** `pipeline::extract` (`pipeline.rs:10-147`) calls `attempt()` up to five times:

- the initial attempt;
- `remove_partial_selectors = false` when there are fewer than 200 words (`:69`);
- the hidden-content retry and the forced hidden-root attempt when there are fewer than 50 words
  (`:77-109`);
- the low-scoring retry (`:111-120`);
- the schema root (`:121-145`).

Each `attempt()` (`pipeline.rs:149-256`) and its `finish()` (`:296-336`) repeat work whose result is
**identical across attempts**, or is **discarded** for losing attempts:

| Repeated work | Location | Why it is redundant |
| --- | --- | --- |
| Full `math::has_math` scan of the clone. This snapshots every node, walks the ancestors of each text node, and runs a regex per text node. | `pipeline.rs:160-164` | For non-math pages, which are all 208 benchmark cases, every attempt rescans a clone that equals `original` at that moment: no cleanup has run yet and `original` does not change between attempts. One scan after preprocessing and adapters gives the same result. |
| `selection::find_main` on the clone. This runs 20 entry-point selectors over the whole document and scores every candidate with about seven subtree traversals (`selection.rs:117-174`). | `pipeline.rs:171-174` | It runs on a fresh clone of the same unmodified `original` with the same `content_selector`, so it returns the same node every time. Images were already removed from `original` at `:32-36`. |
| `schema_element(&clone, …)`, which computes `text()` for every `body *`. | `pipeline.rs:175-179` | Identical across attempts, for the same reason. |
| `select(original.root(), "base[href]")`, a full document traversal. | `finalize.rs:16` via `pipeline.rs:242` | Identical across attempts; one URL could be computed and passed down. |
| HTML serialization and **Markdown rendering**. | `pipeline.rs:308-326` | Every losing attempt renders Markdown when `options.markdown` is set, and the result is then dropped. Only `word_count` and, in one comparison (`:105`), `content.len()` decide the winner. |

TypeScript also re-runs everything per retry, so this behavior faithfully matches the original, but
Rust does not need to repeat it. With Markdown enabled, the Rust port does strictly more work per
retry than TypeScript, which converts to Markdown once after `parse()`.

**Suggested shape.** This change is contained in `pipeline.rs` and adds no new module:

```rust
struct Prepared {            // built once in extract()
    original: Document,      // preprocessed, adapter-mutated, images removed
    warnings: Vec<Warning>,  // pre-preprocess scan, else one post-preprocess/adapter scan
    metadata: Metadata,
    base: Option<Url>,       // page URL joined with the first <base href>
    main: Option<NodeId>,    // find_main + schema-body substitution on `original`
}
struct Candidate {           // one per attempt; owns its cleaned clone
    doc: Document,
    root: NodeId,
    output: Output,          // enum { Unsafe, Outer, Inner } replaces the two bools
    word_count: usize,
    metadata: Metadata,
    debug: DebugInfo,
}
impl Candidate {
    fn html(&self) -> String;                      // lazily, used by the one :105 comparison
    fn finish(self, options: &Options, kind: Option<String>) -> Extraction; // once, for the winner
}
```

- `attempt(&Prepared, &Options, root: NodeId) -> Candidate`. The existing `forced: Option<NodeId>`
  already relies on the needed invariant, as described below.
- This removes the `#[allow(clippy::too_many_arguments)]` on `finish` and the `finish(root, true,
  false, …)` boolean pairs at `pipeline.rs:28-30` and `:193-203`.
- The `from_html` adapter path and the unsafe-selector path produce a `Candidate` the same way.

**Invariant to document.** Every attempt maps `NodeId`s from `original` into its clone. That is
valid because `dom_query::Document` derives `Clone` over an index arena. It remains valid only while
nothing mutates `original` between the moment the ID is chosen and the clone. A newtype
`OriginalNode(NodeId)` with `fn in_clone<'a>(&self, &'a Document) -> NodeRef<'a>` makes the
invariant visible.

**Images on the adapter path.** The repeated image removal at `pipeline.rs:165-169` matters only for
images that a selector adapter added after `:32`. Run it once on `original` after the adapter
returns a selector, instead of once per attempt.

**Expected effect (structural, not timed).** For each `extract` call:

- `find_main`, `schema_element`, the `<base>` lookup, and Markdown rendering drop from up to five
  runs to one;
- math scans drop from up to six to at most two;
- HTML serialization drops to one, plus at most one lazy comparison.

The benchmark selection is retry-heavy: 146 of 191 generic cases have a TypeScript word count below
200, and 41 are below 50. Rust word counts differ slightly, so these numbers are a proxy.

**Validation.**

- The corpus gate must show unchanged native fingerprints.
- Add a counter-based unit test, or a debug-only assertion, that `find_main` and the Markdown
  conversion run once per `extract`.
- The profiling reviewer should re-measure HTML and HTML+Markdown.

**Risk.** Low to medium. Debug `content_selector` and `errors` must still come from the winning
attempt. Keep the existing retry thresholds and comparison order exactly as they are.

---

## S2: public API boundary and error model (P1)

1. **Hidden-public internals.** `lib.rs:8-30` makes `dom`, `extractors`, `markdown`, `math`,
   `metadata`, `safety`, and `utils` public with `#[doc(hidden)]`. Three problems follow:
   - The public signatures expose `dom_query::{Document, NodeRef, Tree, NodeId}`, so a
     `dom_query` upgrade is a semver break for anyone using them.
   - `extractors::extract` returns **unsanitized** adapter HTML (`Extracted.content_html`) and
     mutates the document it receives (see S5). That bypasses the README promise that
     "sanitization runs regardless", although only for callers who reach into hidden modules.
   - `dom::append_fragment` carries a "never interpolate unescaped page strings" precondition that
     only crate-internal code can uphold.

   The tests use about 25 internal functions, for example `dom::select` (41 uses),
   `safety::sanitize` (11), `metadata::extract` (9), and `extractors::extract` (5).

   **Suggested change:** make these modules `pub(crate)`. Expose a test-only facade
   `#[cfg(feature = "internals")] pub mod internals { pub use crate::dom::…; … }` and enable it for
   tests and examples with a self dev-dependency:
   `[dev-dependencies] defuddle = { path = ".", features = ["internals"] }`. This is a mechanical
   import change in seven test files. The public surface becomes `extract`, `html_to_markdown`, and
   `types::*`. `safety::is_dangerous_url` could stay public if it is wanted as a utility.

2. **`Error` is fictional.** `types.rs:116-121` declares `InvalidInput` and `Serialization`, but
   nothing constructs them, and `extract` and `html_to_markdown` can never return `Err`. Meanwhile
   panics do escape (D2). **Suggested change:** keep the `Result` signatures for forward
   compatibility, for example for future resource limits (see H1). Mark `Error` `#[non_exhaustive]`,
   delete the dead variants or replace them with one reserved variant, and document
   "currently infallible; never panics on input" backed by the D2 lint. The alternative, an
   infallible `-> Extraction`, is also defensible because the crate is unpublished. Either way,
   make the code and documentation agree.

3. **Output structs.** `Extraction`, `Metadata`, and `DebugInfo` are consumed rather than built by
   callers, so `#[non_exhaustive]` makes adding fields non-breaking. Do **not** add it to `Options`:
   that would forbid `Options { markdown: true, ..Default::default() }` outside the crate, which is
   the documented usage.

4. **`IncludeReplies::All` behaves exactly like `Extractors`** (`extractors.rs:190-192`). This
   matches TypeScript 0.19.5, where every extractor checks `!== false`. Document it on the enum so
   callers do not expect generic-page replies.

**Validation.** Run `cargo test`, `cargo doc`, the README doctest, and clippy. Check that the only
public items are the intended ones, for example with `cargo public-api` if it is available.
Otherwise no behavior changes.

---

## S3: run cheap predicates first and avoid full-text materialization (P1)

These are semantics-preserving rewrites with no new abstraction. Each one replaces an
O(subtree)-per-node check that runs *before* a cheap discriminating test, or a full `text()` call
that is used only for its first or last character. All of the predicates are pure, and nothing
mutates the tree between evaluating them, so reordering them cannot change results.

| Location | Current | Change |
| --- | --- | --- |
| `removals.rs:88-92` (`remove_hidden`) | For **every** element: `select(node, "math,[data-mathml],.katex-mathml")`, a full subtree scan, then `is_hidden(node)`. | Test `is_hidden(node)` first and run the math-descendant scan only for hidden nodes. Cost drops from O(N·depth) to about O(N). |
| `removals.rs:177-180` (partial selectors) | `closest(code,pre,[data-defuddle])` and `select(node,"pre")`, a subtree scan, for every element with a class, ID, or test attribute, before the regex. | Compute `partial_match` first and apply the code/pre exclusions only to matches. `seen` is updated only after both, so the order of insertion is unchanged. |
| `standardize.rs:584-599` (`remove_empty`) | `node.text()` allocates the full subtree text for every element, then checks that every child is text or `<br>`. | Check the children first. When it holds, the text is just the direct text children. |
| `standardize.rs:796-826` (`whitespace` pairs) | `current.text()` and `next.text()` allocate full subtree text for every adjacent pair at every level, only to inspect the first or last character and emptiness. | Add `dom::first_text_char(node)` and `dom::last_text_char(node)` helpers that walk to the first or last non-empty text descendant. The result is identical because `text()` is the concatenation of descendant text nodes. |
| `standardize.rs:68`, `:97`, `:304`, `:340`, `:398`, `:587`, `:695`, `:737`, `:754`, `:834` | `protected(node)` calls `closest()` with a six-alternative matcher for every text node or element, walking all ancestors. | Use one pruned preorder traversal that does not descend into protected subtrees. Seed it with `closest(root, PROTECTED)` so root-above-protected behavior is unchanged. |
| `markdown.rs:1058` | `source_block` subtree scan per child. | See D1 item 2. |

**Validation.** The corpus gate must keep exact fingerprints, and existing standardize and removal
tests must pass. The profiling reviewer should confirm per-pass gains. The asymptotic improvement is
certain from the code, but I measured no absolute effect.

---

## S4: one home for DOM primitives (P2)

| Helper | Copies | Note |
| --- | --- | --- |
| `tag()` | `utils.rs:62`, `standardize.rs:28` (wrapper), `code.rs:189`, `images.rs:141`, `markdown.rs:684` (`node_tag`) | Each call allocates twice: `node_name()` builds a `StrTendril` (dom_query `node_ref.rs:426`) and the lowercasing builds a `String`. About 80 call sites, many inside per-node loops (24 in `markdown.rs`). |
| retag (`element.name.local = …`) | `standardize.rs:51`, `code.rs:420`, `images.rs:304`, `callouts.rs:35`, `markdown.rs:634` | This is a security-relevant operation: the port contract says to determine unsafe-root identity before retagging. |
| `NodeRef::is(sel)` | 34 uses (`patterns.rs` 8, `comments.rs` 12, `conversations.rs` 7, `metadata.rs` 4, …) | dom_query compiles a new `Matcher` on every call (`node_ref.rs:777-779`), bypassing the bounded cache in `dom.rs:16-37`. Examples are `patterns.rs:79` inside a sibling loop and `metadata.rs:611` per visited node. |
| `contains` | `utils.rs:74`, `footnotes.rs:21` | Equivalent. |
| `re!` macro | `extractors.rs:21-27`, `metadata.rs:9-14` | Identical. |
| word counting | `utils::count_words`, `metadata::words` (`metadata.rs:142-158`) | Byte-for-byte identical logic. |
| Query API | `crate::dom::select` (cached; invalid selector returns empty) vs. dom_query `Document::select` / `select_single` / `Selection::from` (19 uses in `metadata.rs`) | Two query layers with different caching and edge semantics. |

**Suggested change.** Move these primitives into `dom.rs`:

- `dom::retag(node, &'static str)`, with a `debug_assert!` that the target is not one of the tag
  names that `safety::is_unsafe_element` rejects. The `'static` bound proves that retag targets are
  never page-derived. `spans()` (`standardize.rs:306-310`) can map its allowlisted `data-as` value
  to the matching `&'static str` entry.
- `dom::is(node, sel)`, which uses the cached matcher.
- `dom::has_tag(node, &str)`, which compares the local name without allocation. html5ever already
  lowercases HTML tag names; keep the lowercasing comparison only where foreign SVG/MathML names
  matter (`safety.rs:284-287`, `math.rs:454`).
- Move the `re!` macro into `utils.rs` and delete `metadata::words`.

Keep the separate `protected()` sets in `patterns.rs:39` and `standardize.rs:41`; they differ
intentionally. Give them distinct names, for example `pattern_protected` and `format_protected`.

**Validation.** The corpus gate and clippy. The performance effect of removing `tag()` allocations
is a hypothesis for the profiling reviewer.

---

## S5: adapter contract (P2)

1. `Extracted { content_html: Option<String>, content_selector: Option<String>, … }`
   (`extractors.rs:29-35`) can represent neither or both. The pipeline (`pipeline.rs:37-65`) would
   fall through to generic extraction in the first case and ignore the selector in the second. Only
   the constructors `direct()` and `selected()` keep the invariant. **Change to**
   `enum AdapterContent { Html(String), Selector(&'static str) }`; all six `selected(…)` call sites
   pass literals.
2. Selector adapters mutate the shared `&Document` through interior mutability:
   - Substack appends a wrapper to `<body>` (`embedded.rs:86-91`).
   - NYT does the same (`embedded.rs:417-422`).
   - Medium edits `article` in place (`embedded.rs:475-510`).

   Today every mutating path returns `Some`, so generic attempts never see adapter edits. That is
   true only by inspection, not by type. **Change:** state the contract on `extractors::extract`
   ("may mutate `doc`; only when returning a Selector"). Optionally have the materializing adapters
   return the node they built (`AdapterContent::Node(NodeId)`) instead of a marker attribute plus a
   selector round trip.
3. Pass the parsed `&Url` and host to adapters instead of re-parsing (`video.rs:10` with `unwrap`;
   see D2).

**Considered and not recommended:** converting the string-building adapters to direct DOM
construction. They escape at interpolation (`esc`, `link`, `image`), the result is sanitized in
`from_html`, and they are site-specific rather than hot. The rewrite would be large and the benefit
small.

---

## H1: superlinear scaling with nesting depth (measured curve, P2)

HTML-only `extract` through the debug CLI. Another reviewer was running a CPU-heavy job at the same
time, so the absolute numbers are noisy; the trend is consistent.

| Depth | Nested `<div>` | Nested `<span>` inside `<p>` |
| ---: | ---: | ---: |
| 100 | 1.42 s | 0.53 s |
| 200 | 4.98 s | 2.44 s |
| 400 | 23.7 s | 12.7 s |
| 800 | >60 s (timeout) | >60 s (timeout) |

5,000 levels did not finish in 120 s. Each doubling costs about 4.5–5×, which is roughly
O(depth^2.2–2.3) or worse. From reading the code, the likely contributors are:

- per-node ancestor walks with selector matching: `protected()`, `attached()`, `contains()`, and
  `closest()` in `standardize`, `patterns`, and `removals`;
- per-node `text()` and `select()` calls (S3);
- `patterns::wrapper` (`patterns.rs:45-53`), which walks up the ancestors comparing
  `parent.text()` at each step, giving O(depth²) per call;
- the repeated work multiplied by retries (S1).

Wide documents of realistic depth are not affected the same way. This is an availability concern
for adversarial input. S1 and S3 remove much of the cost. If more is needed after profiling,
consider a documented resource guard, such as a maximum processed depth that returns `Err`. That
would give S2's `Error` a real variant. Do not truncate content silently.

**Validation.** The profiling reviewer should attribute time per pass with a depth-scaling series.
Add a non-timing regression, for example 2,000 nested `<div>` elements completing under the test
harness timeout in release, once a target bound is chosen.

---

## H2: removal passes scan `body`, output is `root` (hypothesis, P3)

`pipeline.rs:215-229` passes `body` to `remove_small_images`, `remove_hidden`, `remove_selectors`,
and `remove_low_scoring`. `remove_low_scoring` evaluates `likely_content` and `clutter_score` (many
subtree scans) for every block outside the main content. Only `root` is serialized. Ancestors of
`root` are already exempt (`removals.rs:233`, `:275`), so removing nodes outside `root` cannot change
`content` or the word count. TypeScript does the same over the whole document, so this is parity,
not a defect.

Scoping these passes to `root` when it is inside `body` could reduce the work substantially on
pages with heavy navigation or comments. It has two observable differences:

- `debug.removals` would no longer list nodes outside the root;
- a hidden ancestor of `root` would stay attached, so later `closest()` calls could see above
  `root`.

Treat this as a profiling-driven option, validated by the corpus gate. Do not adopt it on reasoning
alone.

---

## Categories with no actionable finding

- **Ownership and lifetimes.** The `NodeRef<'a>` borrow model is sound. Outputs are owned. There is
  no `unsafe`. The thread-local matcher cache uses `Rc` correctly and is bounded at 512 entries. Every
  selector is a static literal except the user's `content_selector`; I found no page-derived
  selectors, so the cache cannot be churned by page content.
- **Sanitization placement.** `safety::sanitize` runs last on every output path:
  `pipeline.rs:27`, `:192`, `:244`, `:272`, `:282`, and `markdown.rs:24`. The unsafe-root check
  happens before any retag. Base-URL resolution happens before sanitization, so a `javascript:`
  `<base href>` cannot leak through resolved links. No bypass found.
- **File and module size.** The largest files are about 1,100 lines and cohesive. No split is
  warranted. The `standardize` ↔ `footnotes` cycle (`footnotes.rs` calls
  `standardize::remove_orphaned_dividers`) is harmless. If S4 creates a shared cleanup home, move
  that function there.
- **Dependencies.** No new dependencies are needed for any recommendation above.

## O1: optional style (no priority)

- `extractors.rs:8-19`: the `#[path = "extractors/…"]` attributes are redundant; plain `mod comments;` resolves to the same file.
- One-letter helpers `q`, `a`, `t`, `h` (`extractors.rs:196-210`) are hard to grep. Consider
  `first`, `attr`, `text`, and `inner_html`.
- `video.rs:283` and `comments.rs:93-100` pack many statements on one line. Reformatting them would
  help review.

## Suggested sequencing

1. D1 and D2 with regression tests. These are small, local, and produce no fingerprint changes.
2. S3 reorderings. Mechanical and fingerprint-neutral; run the corpus gate after each.
3. S1 pipeline stages. A focused change in `pipeline.rs`, plus `finalize::urls` taking `Option<&Url>`.
4. S2 API boundary and S4/S5 consolidation. Mechanical, mostly import churn.
5. H1 and H2, only if the profiling data supports them.

Every step must keep the native HTML and Markdown fingerprints unchanged. None of these
recommendations justifies updating a snapshot.

## Appendix: diagnostics

Inputs are in `/tmp/defuddle-arch-review/` (temporary). The CLI was `rust/target/debug/defuddle`.

```python
# D1, extract path (punctuation defeats standardize's spacing pass)
inner = "x"
for i in range(d):
    inner = f"<u><em>w{i}(</em><u>{inner}</u></u>"
html = f"<html><body><article><h1>T</h1><p>Intro paragraph with enough words to be content here.</p><p>{inner}</p></article></body></html>"
# run: defuddle FILE           (linear)
#      defuddle FILE --markdown (exponential)

# H1
"<html><body><article><p>Intro …</p>" + "<div>"*d + "deep text here." + "</div>"*d + "</article></body></html>"
```

D2 inputs: `printf '<html><head><title>T</title></head><body><article><h1>Title</h1><p>\xd9\xa3 March 2024</p><p>Body paragraph with enough words to be content for the article here.</p></article></body></html>'`
and the same with `<p>3 Augu\xc5\xbft 2024</p>`. Run with `--json`; both exit with code 101 and the
panic locations given above.

Retry-frequency proxy: I read `rust/benchmarks/2026-10-20/selection.json` (208 cases) and each
case's `expected.wordCount` and `expected.extractorType` from `tests/data/upstream/*.json`.
191 cases have no extractor type; 146 of those have a word count below 200 and 41 below 50.
