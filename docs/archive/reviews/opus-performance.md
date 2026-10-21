# Performance review: profile-backed (Opus 5.5, xhigh)

Scope: warm throughput and cold-call cost of the native crate in `rust/`, at the
benchmarked source. Every `rust/src` hash matches `rust/benchmarks/2026-10-20/summary.json`.
No runtime source, tests, contracts, Cargo files, or TypeScript were changed. All
experiments ran in `/tmp/opus-perf/*` copies, each with its own `CARGO_TARGET_DIR`.

## Result in one table

Each experiment was checked by dumping HTML, Markdown, metadata, warnings, debug
content, and debug removals for all **226** cases (math cases included). Every
experiment was **byte-identical** to the baseline. The cumulative E1–E7 copy passes
all **170** tests (`cargo test --release --locked`). Timings are A/B runs interleaved
in the same window; ratios are more reliable than absolute values (see Noise).

| Rank | ID | Change (all in `rust/src`) | Kind | Measured effect | Size |
| --- | --- | --- | --- | --- | --- |
| 1 | P1 | Bounded, early-exit ancestor walk in the byline loop, plus three pure reorderings in `patterns.rs` | **Proven performance defect** (superlinear) | Corpus −30% (sum of per-case medians 1022 → ~710 ms); lesswrong 357 → 67–83 ms | ~45 lines |
| 2 | P2 | ASCII word boundaries `(?-u:\b)` in `NAV_WORDS` and `CONTENT_DATE` (`selection.rs`) | Measured; also closer to JS `\b` | Additional −8.5% end to end; regex micro-benchmark 43× and 27× faster | 2 lines |
| 3 | P5 | Indexed matching for the 166 `EXACT_SELECTORS` (`removals.rs:153`) | Measured | Additional −10.4% (515 → 458 ms) | ~110 lines |
| 4 | P3 | Memoize `find_main` across retry attempts (`pipeline.rs:173`) | Measured | Additional −5.6% | ~15 lines |
| 5 | P7 | Run the cheap test before the subtree scan in `remove_hidden` and the partial-selector loop | Measured | About −5% (−6.5% together with P6) | ~10 lines |
| 6 | P4 | `NEWSLETTER`: `[\s\S]` → `(?s:.)` (`patterns.rs:26`) | Measured (cold path) | First call 38 → 19 ms; CLI on a small page 37 → 20 ms | 1 line |
| 7 | P6 | `dom::exists()` short-circuit for 47 `select(..).is_empty()` sites | Measured, small; clearer code | About −1.5% | Mechanical |

**All of E1–E7, on the project's own harness** (`examples/benchmark.rs`, the
saved `selection.json`, 7 rounds after 2 warmups, two alternating runs per binary):

| Mode | Baseline median | E1–E7 median | Change |
| --- | ---: | ---: | ---: |
| html | 851.1 / 843.2 ms | 430.9 / 438.1 ms | **−49%** |
| html+markdown | 866.2 / 857.0 ms | 449.2 / 454.4 ms | **−48%** |
| Per-page median (html) | 1.49 ms | 1.10 ms | −26% |
| Per-page p95 (html) | 7.35 ms | 6.43 ms | −13% |
| lesswrong (html) | 285 ms | 46 ms | −84% |
| Cold first call, 319-byte page | 35–38 ms | 18 ms | −50% |

Checksums are identical (`4858911` / `8471124`). No case got more than 5% slower.
The median per-case ratio is 0.77, and adapter cases bypass the changed passes, so
they stay at 1.00×. These are local, warm figures for this corpus. Re-run
`scripts/porting/benchmark.mjs` (both sides) before updating BENCHMARKS.md or
claiming a ratio against TypeScript.

---

## Method and environment

* **Machine:** i7-4870HQ (4C/8T), Linux 7.2.5, `schedutil` governor. Load average
  was about 48 throughout, but `vmstat` showed 85–100% idle CPU. The load came from
  D-state I/O threads, not CPU contention.
* **Toolchain incident (affects other reviewers):** midway through, the shared
  rustup toolchain began crashing. `~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/rustc -vV`
  and `rustdoc` segfault inside `ld-linux` (kernel log: `segfault at 8 … in ld-linux-x86-64.so.2`).
  `readelf -d lib/librustc_driver-7ed0375ae352bd75.so` reports a dynamic section
  with **one NULL entry**, so the file contents or their page cache are corrupted.
  Its mtime is still 12:12 and nothing in pacman's log changed. I did not modify
  `~/.rustup`. For all measurements I installed the same toolchain,
  rustc 1.99.1 (`58cf8f9c3`), into `/tmp/opus-perf/rustup-home`. **The default
  toolchain probably needs `rustup toolchain uninstall stable && rustup toolchain install stable`.**
* **Writes outside `/tmp`:** a failed attempt to install `samply` downloaded crate
  sources into the default Cargo registry cache (`~/.cargo/registry`). Nothing else
  outside `/tmp` was written except this report.
* **Profilers:** `perf` is not installed, and `samply` needs
  `perf_event_paranoid ≤ 1` (it is 2). Instead I used: (1) RAII phase timers and
  section markers in a `/tmp` copy of the pipeline; (2) counters for `select()`
  calls, descendants visited, matcher-cache hits, and `.is()` calls; (3) targeted
  micro-benchmarks over the real corpus texts and attributes; (4) end-to-end A/B on
  the full 208-case benchmark selection.
* **Workload:** the saved `selection.json` (208 non-math cases, 1,372,181 bytes),
  with exact URLs and default options. Most A/B runs used a per-case harness: one
  warmup, then the median of 7 iterations per case, summed. Final numbers use the
  project's own `examples/benchmark.rs` (source hash unchanged), interleaved.
* **Noise:** the baseline's absolute time drifted from about 1016 ms to 850 ms
  over the session (frequency/thermal state). Each comparison therefore ran
  base and experiment back to back, two or three times. Run-to-run spread within a
  window was about 1–2% (for example E1–E5: 460.4 / 461.9 / 460.6 ms).
* **Quality checks for every experiment:** a full dump of `extract()` with
  `markdown: true` plus a `debug: true` run per case (content, debug content,
  removal records, selector path, metadata, warnings), all 226 cases, compared with
  `diff -r` against the baseline dump. All seven experiments were byte-identical.
  The test suite passed (170/170) on E1–E4, E1–E5, and E1–E7. It was run in
  release mode: a debug-mode run hit the `/tmp` tmpfs quota during linking (an
  environment failure, not a test failure). Clippy and rustfmt are not in the
  minimal toolchain and were **not** run on the experiment code.

## Where the time goes (baseline)

Phase profile of one corpus pass (instrumented build, HTML mode, 3 iterations;
instrumentation adds about 5%):

| Phase | ms/pass | Share | Notes |
| --- | ---: | ---: | --- |
| `patterns::remove_patterns` | 369–376 | 40% | Byline loop alone: 261–268 ms (lesswrong 230 ms) |
| `selection::find_main` | 132–142 | 15% | `CONTENT_DATE` regex alone: 52 ms |
| `standardize::content` | 116–124 | 13% | `whitespace` 43–47, nbsp pass 15, `code` 10, `remove_empty` 10 |
| `removals::remove_selectors` | 90–95 | 10% | Exact list: 74–78 ms |
| `before_removals` (footnotes/callouts) | 28–32 | 3% | Footnotes 26 ms |
| HTML parse (`Document::from`) | 26–28 | 3% | |
| Metadata | 23–25 | 3% | Includes ~7 ms of per-document regex compiles |
| `remove_hidden` / `remove_low_scoring` | 19–23 each | 2% each | |
| Serialization / word count / sanitize | 8.7 / 4.0 / 1.7 | 1.5% | |
| Markdown (html+md minus html) | 15–26 | 2–3% | From the harness's mode difference |

* **Concentration:** one page, lesswrong (146 KB), was **32.6%** of the saved Rust
  corpus time, and it was the only page where Rust was slower than TypeScript (327
  vs 270 ms). Excluding it, the saved Rust/TS ratio is 5.36× rather than 3.85×.
* **Fixed costs on tiny pages:** a 319-byte page takes 0.7 ms; 758-byte
  `bootstrap-alerts` takes 3.2 ms. Small fixtures often run 3–5 attempts (the
  `< 200`/`< 50` word retries), and every attempt re-runs selection, removal
  passes, and standardization on a fresh clone. That cost is algorithmic parity
  with TypeScript; P3 removes the repeated `find_main`.
* **Selector cache:** it works as intended. Steady state had zero misses and zero
  `clear()`s (512 bound) across 196k lookups per pass. Each lookup still hashes the
  selector string (≈40–130 ns, ~10 ms/pass); see Hypotheses.

---

## P1 — Superlinear ancestor walk in the byline loop (proven defect, highest priority)

**Where:** `src/patterns.rs:372-378` (loop starts at `:362`).

```rust
if std::iter::once(node)
    .chain(node.ancestors_it(None).take_while(|p| p.id != root.id))
    .any(|n| {
        words(n) <= 15                       // words(n) = count_words(&n.text())
            && re!(r"(?i)^(?:date|published|…)\s*:").is_match(n.text().trim())
    })
```

**Defect:** this runs for every `p,span,div,time` node with at most 15 words, which
is most inline spans. For each one it walks **every** ancestor up to the root, and
`words(n)` builds the full subtree text of each. Ancestors near the root contain
the whole article, so the cost is O(qualifying nodes × depth × article text). On
lesswrong this one loop takes 230 ms of a ~300 ms page. It is what makes the page
slower than TypeScript.

**Change (verified):** ancestor text contains the node's text as a contiguous
substring, so the word count can only grow toward the root. Distinct words of a
substring stay distinct in the superstring, and separators are preserved.
Stopping at the first ancestor over 15 words is therefore exactly equivalent.
Counting with an early exit avoids building large strings at all:

```rust
// utils.rs: same classification and cross-text-node state as count_words(&node.text())
pub fn words_at_most(node: NodeRef<'_>, limit: usize) -> bool { /* walks once(node).chain(node.descendants_it()), stops when count > limit */ }

// patterns.rs
// Ancestor text contains the node's text, so word counts only grow.
.take_while(|n| words_at_most(*n, 15))
.any(|n| re!(r"(?i)^(?:date|published|updated|posted|from|to|subject)\s*:").is_match(n.text().trim()))
```

`descendants_it()` and `text()` share dom_query's `descendant_nodes` preorder, so
the bounded count equals `count_words(&n.text())` up to the limit.

The same family includes three pure reorderings. Each evaluates the cheap
predicate before the O(subtree) or O(content) one; the conjunctions themselves are
unchanged:

* `patterns.rs:403`: `pos(node) <= 300 && re(^current time in$)` → regex first. `pos()` does a `content_text.find` plus a UTF-16 prefix length per node.
* `patterns.rs:681-684`: reuse `text` for the word count (it was computed twice), and evaluate `pos(node) > 200` after the regex.
* `patterns.rs:605-607`: check the 2..=limit word range before `replace_all` over the subtree text. The `NEWSLETTER` match is unchanged.
* `patterns.rs:154-161` (`content_start`): stop at the first prose leaf instead of classifying every block with `prose()`. Only the first candidate and the first leaf are used.

**Evidence:**

| Run (interleaved) | Base | E1a (walk) | E1a+E1b |
| --- | ---: | ---: | ---: |
| Corpus, sum of medians | 1028.98 / 1015.96 | 745.58 / 737.66 (0.725×) | 709.70 / 610.19 vs base 1015.26 / 879.92 (0.696×) |
| lesswrong | 360 / 355 | 97.8 / 96.2 | 82.8 / 67.4 |

Outputs were identical for all 226 cases.

**Validation needed:** cargo test, clippy, and fmt. Add a unit test that
`words_at_most(n, k) == (count_words(&n.text()) <= k)` over all elements of a few
corpus documents, including CJK and words split across inline elements.
**Confidence:** high, both for equivalence and impact.

## P2 — Unicode `\b` forces the slow regex engine on article-sized text (measured)

**Where:** `src/selection.rs:95-100` (`NAV_WORDS`, used at `:361` in
`clutter_score` on every block's subtree text) and `:103-104` (`CONTENT_DATE`,
used at `:130` in `score` on every entry-point candidate's text, including
`body`).

**Cause:** the regex crate's lazy DFA cannot evaluate a Unicode word boundary on
non-ASCII input. It falls back to the PikeVM for the whole haystack. Typographic
quotes, nbsp, and dashes make most article text non-ASCII.

**Evidence:** micro-benchmark over the `body.text()` of all 208 cases (450 KB, 63
of them non-ASCII), with identical results:

| Regex | Unicode `\b` (current) | `(?-u:\b)` | Differences |
| --- | ---: | ---: | --- |
| `NAV_WORDS` RegexSet | 145.1 ms/pass | 3.4 ms | 0 cases |
| `CONTENT_DATE` | 19.6 ms/pass | 0.73 ms | 0 first-match ranges |

End to end on top of P1: the ratio goes from 0.696× to 0.637× of base (−8.5%).
lesswrong goes 296 → 51 ms, measured as base against P1+P2 in the same window.
The 226-case dump is identical.

**Fidelity:** the TypeScript sources use JavaScript `\b`, which is ASCII-only:
`navigationIndicatorRegexes` (`new RegExp('\\b…\\b')` on lowercased text) and
`contentDatePattern` (`/\b…\b/i`) in `src/removals/scoring.ts`. The ASCII boundary
therefore matches the reference more closely. A difference is only possible when
an indicator abuts a non-ASCII letter (for example `menué`). Optionally apply
`(?-u:\b)` to the other `\b` patterns for consistency. They run on texts of 60
words or fewer, so there is no performance reason to.
**Confidence:** high.

## P5 — The 166-selector exact list costs ~3.6 µs per element (measured)

**Where:** `src/removals.rs:15,153` (`select(root, &EXACT)` over `body`, once per
attempt).

**Evidence:** dom_query evaluates a selector list one selector at a time, with a
tree borrow and attribute scans for each. Over all 16,091 corpus elements:

| Subset | Selectors | ns/element |
| --- | ---: | ---: |
| Tag only | 19 | 273 |
| Class only | 28 | 723 |
| Id only | 30 | 543 |
| Single attribute | 52 | 1290 |
| Compound | 34 | 574 |
| `:has` | 3 | 87 |
| **All (joined)** | 166 | **3595** |

**Change (verified):** `dom::SelectorIndex` keeps one `Matcher` per selector and
buckets each by a necessary key from its rightmost compound: tag, class, id, or
attribute name. Selectors with no plain key, a top-level comma, an escape, or a
namespace go to an "always" list. Per element, it gathers candidates from the
element's tag, id, classes, and attribute names, then runs **the real matcher**
on each. The bucket is only a superset filter, so results equal the joined list:
the same nodes in document order. Tags and attribute names are compared
lowercased (a superset, since HTML names are case-insensitive). Classes and ids
are compared exactly, because dom_query always matches in `NoQuirks` mode.

**Effect:** with P1–P4 applied, 515 → 458 ms (−10.4%). Outputs identical for 226
cases. Building the index costs about 0.2 ms, once per thread.

**Cost:** about 110 lines in `dom.rs`. The selector-key scanner is the risky part.
**Validation needed:** a property test asserting `index.select(root) == select(root, joined)`
over every corpus document, plus synthetic selectors (combinators, `:not(a, b)`,
attribute operators containing `~`/`|`, quoted brackets). **Confidence:** high
for this list. Generalizing it is a hypothesis (H2).

## P3 — `find_main` is recomputed on every retry (measured)

**Where:** `src/pipeline.rs:173`, called from `attempt` (`:149`). Retries are at
`:69`, `:75`, `:83`, `:120`.

Every attempt clones the same `original`, which is never mutated between
attempts. `remove_images` is applied before any attempt. Retries change only
removal flags, never `content_selector`. `find_main` (22 full-document scans plus a
`score()` per candidate) therefore returns the same `NodeId` every time. The code
already relies on node IDs being stable across clones (`forced` hidden or schema
roots).

**Change (verified):** a `OnceCell<Option<NodeId>>` created in `extract()` and
shared by the default attempt and all retries. The adapter selector attempt
(`:55`) gets its own cell. Document the invariant next to the cell.
**Effect:** 0.637× → 0.601× of base (−5.6%). Outputs identical; 170/170 tests
pass. The `schema_element` fallback in `attempt`, an O(n × subtree) text scan,
could share the same memo; that part is not measured.
**Confidence:** high.

## P7 — Subtree scans evaluated before cheap predicates (measured)

* `src/removals.rs:88-92`: for **every** element, `select(node, "math,[data-mathml],.katex-mathml")`
  scans its subtree before the cheap `is_hidden(node)` check, which is
  O(n × subtree). Test `is_hidden` first.
* `src/removals.rs:177-182`: `closest(..)` plus `select(node, "pre")` (a subtree
  scan) run for every class- or id-bearing node before the partial-attribute
  regex. Move them after `partial_match`.

Both are pure predicates, so each reordered conjunction is equivalent. Debug
removal records are unchanged, as the debug dump confirms.
**Effect:** with P6, 460.6 → ~431 ms (−6.5%, three interleaved runs); P7 is about
5 points of that. **Confidence:** high.

## P4 — Cold start: one regex costs 19 ms to compile (measured)

**Where:** `src/patterns.rs:26` (`NEWSLETTER`).

The first call in a process compiled about 35 ms of lazy regexes. `NEWSLETTER`
alone took **19.5 ms**: under `(?i)`, each of the 240 bounded repetitions of
`[\s\S]` produces a case-folded union of all of Unicode. `(?s:.)` denotes exactly
the same set (any scalar value) without the folding:

| Variant | Compile time | Agrees on samples |
| --- | ---: | --- |
| Current | 19.41 ms | — |
| `(?s:.)` instead of `[\s\S]` | 1.35 ms | yes |

**Effect:** first `extract()` call 35.6 → 19.4 ms on a 319-byte page. CLI process
time (spawn, read, extract, print) on small page 37.0 → 20.1 ms, and on Wikipedia
64.2 → 45.1 ms. No warm effect. Outputs identical for 226 cases.

Remaining cold cost (~18 ms), for reference: `PARTIAL` 6.4 ms, `PARTIAL_ANCHORED`
4.5 ms (only compiled once some id is non-delimited), the contact regex 1.2 ms,
and others under 1 ms each. See H5. **Confidence:** high.

## P6 — `select(..).is_empty()` builds a vector to test existence (measured, small)

There are 47 call sites, including 25 in `patterns.rs`. They collect every match
(scanning the whole subtree) only to test non-emptiness. A
`dom::exists(root, selector)` built on `descendants_it().any(..)` stops at the
first match and states the intent. The replacement is mechanical:
`!select(a,b).is_empty()` → `dom::exists(a,b)`.

**Effect:** about −1.5% (458 → ~453 ms, close to the noise floor). The output dump
is identical. Recommended mainly as a clarity improvement. There are ~29 similar
`.first()`/`.len()` uses: `.first()` sites could use a `dom::first` with early
exit; `.len()` sites need the count. **Confidence:** high for equivalence,
moderate for the size of the gain.

---

## Remaining profile after P1–P4

Instrumented build, 542 ms/pass. P5 and P7 later shrink `rs:exact` and
`remove_hidden`.

| Phase | ms | Phase | ms |
| --- | ---: | --- | ---: |
| `standardize` (`whitespace` 43, nbsp 15) | 115 | `before_removals` (footnotes 25) | 30 |
| `patterns` (`content_start` 17, byline 17, boilerplate 13, newsletter 11) | 100 | parse | 26 |
| `remove_selectors` (exact 68) | 87 | metadata | 23 |
| `find_main` (score 36, entry scans 17.5) | 54 | `remove_hidden` / `low_scoring` | 22 / 21 |

## Reasoned hypotheses (not verified end to end)

| ID | Location | Measured hotspot | Suggestion | Expected | Confidence |
| --- | --- | --- | --- | --- | --- |
| H1 | `standardize.rs:41` `protected()`, called per text node at `:68`, `:737`, the final loop, and per element at `:754` | ~25–30 ms/pass (`ws:protected_text1` 9.0 + `ws:protected_el` 6.3 + final and nbsp passes) | One top-down traversal that does not descend into `pre,code,svg,math,mjx-container,.katex` subtrees. First check whether `root` itself has a protected ancestor, because `closest` looks above `root`. | −4 to −5% | Medium |
| H2 | `selection.rs` `score` (`FOOTNOTE_INLINE_REFERENCES`, 51 selectors; `FOOTNOTE_LIST_SELECTORS`, 29) and the 22 `ENTRY_POINT_ELEMENTS` document scans (`:183`) | 13 + 9.5 + 17.5 ms/pass | Use `SelectorIndex` (P5) automatically inside `dom::matcher` for lists above about 16 selectors. Do the entry-point pass as one traversal that records the first matching index per element. | −4 to −6% | Medium |
| H3 | `standardize.rs:807-808` `current.text()` / `next.text()` per sibling pair, to read one boundary character | 2 ms on this corpus; O(n × depth) worst case | Helpers that return the first or last non-empty character via an early-exit text walk | Small now; caps the worst case | Medium |
| H4 | `metadata.rs:716`: two `Regex::new` per document in `clean_title` | ~0.1 ms per compile, ≈7 ms/pass (139 compiles over 416 calls) | Compile one static separator regex and compare the site name with case-insensitive prefix/suffix checks. Must preserve `(?i)` Unicode folding (`to_lowercase` comparison) and `regex::escape` semantics. | −1.5% | Medium |
| H5 | `removals.rs:16-31` `PARTIAL`/`PARTIAL_ANCHORED` (544 alternatives, of which 540 are literals) | 6.4 + 4.5 ms cold compile | Build an `aho-corasick` automaton with ASCII case-insensitivity (1.66 ms) plus a 4-pattern residual regex (0.60 ms). For the anchored variant, use a lowercase `HashSet` lookup. ASCII folding matches JS non-`u` `/i`. Note `cookie.law` keeps `.` as "any char". This is a new **direct** dependency, already in the lockfile through `regex`. | −8 ms cold | Medium |
| H6 | `dom.rs:22-36`: the matcher cache hashes the selector on every call (196k/pass, avg 56 bytes) | ≈10 ms/pass | Hoist `matcher()` out of per-node loops (for example `closest` in loops), or key static selectors by `(ptr, len)` | −1 to −2% | Medium |
| H7 | dom_query `NodeRef::is()` (34 call sites) compiles a new `Matcher` on every call | 3,531 calls/pass, 2.9 ms | Route through cached `utils::matches`. The win grows with comment-heavy adapter pages (`comments.rs:96-97` runs per inline node) | −0.5% | High (small) |
| H8 | `utils::tag()` allocates a `String` per call | 116k calls/pass × ~32 ns ≈ 3.7 ms | Only when touching the code: an `is_tag(node, "p")` that compares `qual_name_ref().local` | <1% | High (small) |

## Categories with no actionable finding

* **Markdown renderer:** 15–26 ms per corpus pass (2–3%) in every harness run.
  No hotspot. In my runs, html+markdown was consistently 15–26 ms slower than html,
  so the saved baseline showing HTML+MD faster is noise, as BENCHMARKS.md already
  says. Note that the saved HTML rounds are bimodal (928–1030 ms versus
  1213–1297 ms for rounds 3–5), which inflates the saved HTML median.
* **Parsing, serialization, sanitization, cloning:** 3%, 1%, 0.2%, 0.2%. No
  action.
* **Selector cache design:** the 512-entry bound is never hit, and in steady
  state there are no misses or clears. No change needed beyond H6.
* **Memory/allocation profiling:** not done; no heap profiler is available. The
  allocation-heavy patterns found (`text()` materialization, `select` vectors,
  `tag()` strings) are covered above.
* **Security:** none of P1–P7 touches sanitization, URL filtering, or escaping.
  Sanitization stays unconditional. The 226-case dump comparison includes
  security-fixture outputs, and the corpus test's security reparse passed.

## Recommended order and follow-up

1. Apply P1, P2, P4, and P7. They are small and their equivalence can be argued
   locally. Together they give most of the gain.
2. Apply P3 with an invariant comment. Apply P6 as a readability pass.
3. Apply P5 with the property test described above. Consider H2 next, building
   on the same index.
4. Re-run `cargo test`, `clippy -D warnings`, `fmt --check`, the corpus report, and
   `node scripts/porting/benchmark.mjs rust/benchmarks/<date> 7 2`. Then regenerate
   BENCHMARKS.md, including the TypeScript side, before restating speedups.
5. Add a pathological-input regression check: for example, extraction time on a
   page whose body is repeated k times should stay roughly linear. With E1–E7,
   Wikipedia's body ×1/×2/×4/×8 takes 13.6 / 26.7 / 52.5 / 105 ms; the baseline
   takes 16.0 / 38.9 / 84.4 / 176.7 ms. Repeating lesswrong changes which root is
   selected, so it is not a valid scaling probe.

## Reproduction

The scratch area is `/tmp/opus-perf/` (temporary). The full experiment diff
against the benchmarked source is `/tmp/opus-perf/e1-e7.patch` (324 changed
lines). Review-only harnesses live in each copy's `examples/`: `caseprof.rs`
(per-case medians), `dump.rs` (226-case output dump), `cold.rs`, `scale.rs`,
`rebench.rs`, and `newsletter.rs`. Typical commands:

```bash
source /tmp/opus-perf/env.sh   # private rustc 1.99.1 (58cf8f9c3)
(cd /tmp/opus-perf/rust-exp7 && CARGO_TARGET_DIR=/tmp/opus-perf/target-exp7 cargo build --release --locked --offline --example benchmark --example dump)
/tmp/opus-perf/target-exp7/release/examples/benchmark rust/benchmarks/2026-10-20/selection.json /tmp/out.json
/tmp/opus-perf/target-exp7/release/examples/dump /tmp/out-dir && diff -rq /tmp/opus-perf/out-base /tmp/out-dir
```

The essential non-mechanical hunks of P1–P5 and P7 are quoted in the sections
above. P6 is the mechanical `select(..).is_empty()` → `dom::exists(..)` rewrite.
