# Hardening performance and benchmark review, round 2 (Opus 5.5, xhigh)

Scope: R1 follow-up for
[hardening-opus-performance-r1.md](hardening-opus-performance-r1.md). It checks
that the R1 performance changes are equivalent, and that the benchmark harness
and its evidence are statistically and provenance-wise sound. It ends with a
go/no-go for the quiet final run. Correctness of the in-flight Markdown renderer
belongs to the correctness review.

No source, contract, test, harness, Cargo, or upstream file was modified. The
only writes are this report and scripts/probe inputs under
`~/.cache/defuddle-port-hardening/opus-perf-review-r2/`. No timer, profile,
build, or test suite was run. Smoke timings were used only to check the
harness's arithmetic. No timing value from them appears here.

## Verdict

- **The performance changes are exact.** R1 findings 1.1 (`protected()`) and
  1.2 (lazy schema root) are closed. The changes in 1.3 that were made are exact
  too. The two 1.3 items that were not made are declared and optional.
- **All harness findings (2.1–2.7) are closed.** I recomputed every summary
  statistic, A/A threshold, flag list, and changed-proof list from the smoke raw
  files with independent code; all match exactly. The current and baseline
  executables are each tied to their own source tree.
- **No-go right now.** `rust/src/markdown.rs` changed at 00:53:51, during this
  review: frozen `d0509288` became `967da51e`. `rust/tests/markdown.rs` was
  still changing at 00:56:32. This is the general renderer fix assigned after
  the interrupted correctness review (`hardening-opus-correctness-r2-status.md`).
  The prebuilt executable `b803a9ca…`, the smoke run, the 233-test and
  corpus-gate results, and `frozenSourceHashes` all describe the previous tree.
- **Go once the checklist in section 6 holds.** No harness change is required
  for a go. The only medium item is how the report interprets load averages
  (N2). It needs a documentation note, plus optional CPU-idle evidence.

---

## 1. Snapshot reviewed

The SHA-256 prefixes below were taken at 00:56. Every runtime file except
`markdown.rs` matches `frozenSourceHashes` in `hardening-run.json`. All 28
`rust/src` files combine to path+hash digest `93c1f648de113f8c`.

| File | SHA-256 prefix | Notes |
| --- | --- | --- |
| `scripts/porting/benchmark.mjs` | `5241861e006cfc0e` | |
| `scripts/porting/benchmark.shared.mjs` | `23a5cb6d6e4c23ce` | |
| `scripts/porting/benchmark-checks.mjs` | `55114bad11b09ed9` | |
| `scripts/porting/benchmark-native-repeat.mjs` | `c25832cdf0f13f66` | |
| `scripts/porting/benchmark.oracle.ts` | `eed7b13e51acb77c` | Same as R1 |
| `scripts/porting/benchmark.config.ts` | `2c7695d787d19f0d` | |
| `rust/examples/benchmark.rs` | `4354908ace377154` | Same as R1 (timer code unchanged) |
| `rust/src/standardize.rs` | `1afb27a1c30f899f` | Frozen |
| `rust/src/math.rs` | `d951f3119d6f8961` | Frozen |
| `rust/src/removals.rs` | `0001d66520358aa3` | Frozen |
| `rust/src/pipeline.rs` | `c4a24058422ba947` | Frozen |
| `rust/src/dom/selectors.rs` | `c34238032d1f4b5f` | Frozen (test only) |
| `rust/src/markdown.rs` | `967da51e33499e40` | **Changed during review; frozen `d0509288`** |
| `rust/Cargo.toml` / `Cargo.lock` | `4df75104…` / `9a31f680…` | Identical to the baseline fork |
| `rust/reviews/hardening-benchmark-harness.md` | `7dd7527306312c19` | Changed after smoke (doc only, declared) |
| `vitest.config.ts` / `tsconfig.json` | `175ccc79…` / `f1814552…` | Tracked, unmodified |

`git diff --name-only HEAD` is empty. Every TypeScript source, test,
expectation, and package file therefore equals HEAD. This is how I checked the
"67 original hashes unchanged" claim.

Compared with the R1 probe copy, apart from instrumentation, only six runtime
files differ:
- `standardize.rs`, `math.rs`, `removals.rs`: the 1.1/1.3 changes.
- `pipeline.rs`: 1.2 plus the trim fix.
- `selectors.rs`: the tripwire test.
- `markdown.rs`: in-flight; see N1.

`lib.rs` differs only by the R1 probe hooks. Every other file is byte-identical
to what R1 verified. R1 section 3's proofs therefore still hold for the bounded
counters, `exists`, the selector index, the reorderings, prepared retries, and
winner-only finish.

## 2. R1 obligations

| R1 item | Status | Evidence |
| --- | --- | --- |
| 1.1 F2 reorderings (NBSP, `remove_empty`, final punctuation) | **Closed, exact** | `standardize.rs:80`, `:603-616`, `:892`; section 3 |
| 1.1 `preserve_wrapper` cheap-first | **Closed, exact** | `standardize.rs:416-418` |
| 1.1 F1 one walk, static matchers | **Closed, exact** | `standardize.rs:39-62`, `math.rs:24-31` |
| 1.2 lazy schema root | **Closed, exact** | `pipeline.rs:22-43`, `:273-304`, `:244`; section 3 |
| 1.3 allocation-free class comparison | **Closed, exact** | `math.rs:66-124` |
| 1.3 short-circuit source descendant | **Closed, exact** | `math.rs:35-40`; `removals.rs:91`; `standardize.rs:628` |
| 1.3 `words_at_most` in pattern climbs | Declared not done (optional) | `patterns.rs` unchanged since R1 |
| 1.3 regex-free site-name comparison | Declared not done (optional) | Huge-site compile cost remains, unmeasured |
| 1.3 schema trim before take-100 | **Closed**, matches `src/defuddle.ts:714-715` | `pipeline.rs:442-444`; fail-before/pass-after fixture |
| Selector-index quirks tripwire | **Closed** | `dom/selectors.rs:218-226` |
| 2.1 small-sample p95 | **Closed** | `benchmark.shared.mjs:108-113`, `:138-141`; the smoke summary has no p95 key at n<20 |
| 2.2 automated, validated native repeat | **Closed** | `benchmark-native-repeat.mjs`; section 4 |
| 2.3 discarded real-worker smoke | **Closed for the previous tree**; **must be repeated** (N1) | `benchmark-smoke-r2/`; both logs exit 0 |
| 2.4 private executable copies, re-hashed | **Closed** | `benchmark.mjs:55-61`, `:87-93`; `benchmark-native-repeat.mjs:38-40` |
| 2.5 provenance gaps | **Closed**; polish in N3 | `benchmark.mjs:34-78` |
| 2.6 reference labeling, GC note | **Closed** | `benchmark.mjs:169-170`; method report |
| 2.7 pre-build, governor, README | **Closed** (README replacement pending the final run, as required) | `rust/README.md:157-158` and `BENCHMARKS.md:3-8` are still historical; no smoke figure appears in any document |

## 3. Equivalence of the runtime performance changes (static)

**`protected()` (`standardize.rs:43-62`).** The old expression was
`closest(pre,code,svg).is_some() || closest(SOURCE).is_some_and(is_math_source)`.
`dom::closest` (`dom.rs:76-81`) walks `once(node).chain(ancestors_it(None))` over
elements, which is the sequence the new walk filters.

The new walk returns true at the first code match, so it covers the left
disjunct. It evaluates `is_math_source` only on the first element that matches
`SOURCE`, then continues looking only for code, which is exactly the right
disjunct. When one element matches both selectors, the code branch fires first,
and the result is still true. Both matchers are `Matcher::new` of the same
static strings that `dom::matcher` compiled, so the selector semantics are
identical.

The new oracle test (`standardize.rs:1146-1157`) checks every node of
`dom::subtree`, including template fragments, against the old expression.

**Reorderings.** Each predicate is a pure read at evaluation time:
`attached`, an allowed-tag lookup, child kinds, `text().trim()`, and
`protected`. So `A && B` equals `B && A`.

The final punctuation loop now skips `set_text` when `cleaned == text`. The
skip is exact because dom_query's `set_text` on a `Text` node only replaces
`contents` (`dom_query-0.28.0/src/dom_tree/ops.rs:433-435`). An identical
replacement is a no-op, and the loop only visits text nodes.

**`has_source_descendant` (`math.rs:35-40`)** is equivalent to
`select(node, SOURCE).into_iter().any(is_math_source)`. It uses the same
`descendants_it` and the same exclusion of the root and template content,
because `select` never returns `None` for a valid static selector.

**ASCII comparisons (`math.rs:93-111`).** The prefixes and suffixes are ASCII.
`eq_ignore_ascii_case` on byte slices compares non-ASCII bytes exactly,
`to_ascii_lowercase` leaves them unchanged, and a byte-range `get` cannot panic
on a char boundary. The results therefore equal the former
`to_ascii_lowercase().starts_with/ends_with/==`. The added test covers
multibyte and short strings.

**Lazy schema root.** The cached value depends only on `prepared.original` and
`prepared.metadata.schema_org_data`. Neither changes after `prepare`:
- `find_main` and `selection.rs` contain no mutation calls.
- `attempt` clones first (`pipeline.rs:315`).
- `schema_element` sanitizes its own clone.
- The hidden branch only reads.
- `remove_images` still runs before `prepare`, as before.

The cache therefore holds the value the eager code computed, whenever it is
filled. A miss is cached as `Some(None)`. `pipeline-before.rs` differs from the
current file only by the `OnceCell`, the trim, and the tests.

**Evidence check (not just the reports).**
- The standardize proof's "after" sources for `math.rs`, `removals.rs`, and
  `standardize.rs` match the current hashes exactly. Its before/after JSON files
  hold 452 records and both have SHA-256 `7f82d184…`.
- The schema proof's final `pipeline.rs` equals the current file. Both of its
  226-file comparisons are hash-identical.
- The two proofs change disjoint files. Their combination is covered only by
  the root's integrated corpus gate, which must be re-run after N1 in any case.

## 4. Harness verification performed

- `node --test scripts/porting/benchmark-checks.mjs`: **7/7 passed**.
  `benchmark.mjs --validate` found 208 cases and 1,372,181 bytes, with manifest
  `0e137fc4…` and identity `e26a1dbb…`.
- **Independent recomputation.** I recomputed the following from the smoke raw
  files with separate Python (`recompute_main.py`, `recompute.py`); all values
  matched to 1e-12:
  - `rust-pass-*.json` and `typescript-pass-*.json` give the paired summary:
    corpus speedups, page-median speedups, page p95, extraction-only medians,
    either-pass slower lists, and the absence of small-sample p95.
  - `native-repeat-*.json` gives the corpus ratio, per-pass and pooled per-page
    ratios, A/A p95 fold, threshold, both flag lists, the slower list, and
    `changedOutputProofs`.
  - `cold.json` holds 4 × rounds samples, with no p95.
- **Executable identity.**
  - **Current binary:** the smoke `executableSha256` equals the hash of
    `target/release/examples/benchmark` (`b803a9ca…`). The binary was built at
    00:37:26–00:38:09, after the newest source mtime and ctime (00:35:56). The
    ctime check rules out mtime-preserving copies.
  - **Baseline binary:** `98dfbb11…`, built at 00:05:24, after the fork's newest
    ctime (00:04:03).
  - **Embedded manifest paths:** each binary embeds its own crate path
    (`…/Work/defuddle/rust` and `…/benchmark-baseline-worker`). Each therefore
    reads its own input copy, and both copies are hash-validated against the
    same selection.
  - **Toolchain and profile:** both binaries carry
    `rustc version 1.99.1 (58cf8f9c3 2026-10-13)`. The fork's `Cargo.toml` and
    `Cargo.lock` are byte-identical to current, so both use the same profile
    (thin LTO, `codegen-units = 1`) and the same dependencies.
- **Provenance paths.**
  - Cargo discovers configuration from the current directory upward, and the
    build runs with `cwd = root`. The walk from `rust/` upward is a superset of
    that search, and `$CARGO_HOME/config*` is included.
  - `rust-toolchain*` files are moot, because `PATH` points directly at the
    1.99.1 toolchain instead of rustup proxies. They are hashed anyway.
  - Environment hashes cover `CARGO_*` and `RUST*`, including `RUSTFLAGS`,
    `RUSTC_WRAPPER`, and `RUSTUP_*`. Names suggesting secrets are excluded.
  - No configuration file exists today, and the smoke provenance confirms it.
- **Race windows.** Source, input, and binary hashes are taken before and after
  the build and around every worker. Each worker re-validates every input hash
  itself. The remaining window, a change reverted within one worker, cannot
  affect the copied binary or the inputs. It could affect Vitest's transform of
  `src/`, but that is not credible in a quiet window.
- **Repeat consistency.** The repeat re-verifies that:
  - the original runtime matches the historical hashes;
  - the worker and `math.json` are identical;
  - the paired-run source, harness, and input provenance is unchanged, and new
    files are detected;
  - both binaries keep their hashes.

  It refuses to overwrite evidence. **Do not edit
  `hardening-benchmark-harness.md` or `scripts/porting/*` between the paired run
  and the repeat**; the repeat aborts if they change (fail-safe).
- **TypeScript reference.** The Vitest alias `./elements/math → math.full.ts`
  matches the published Node build: `src/elements/math.ts` re-exports `math.full`
  under `tsconfig.node.json`. The reference is therefore not handicapped beyond
  its documented "source under Vitest/Vite SSR" label.
- **Timers.** `benchmark.rs` and `benchmark.oracle.ts` are byte-identical to the
  files R1 checked. Placement, result sinking, and post-timer validation are
  unchanged and sound. The pre-timer allocations (`outputs`, `case_ms`) stay
  outside the timer.
- **A/A and flag logic** (`benchmark-native-repeat.mjs:98-110`).
  - **Schedule:** B·C | C·B places each baseline/current pair adjacent within its
    pass. Requiring "slower in both passes" cancels linear thermal drift, and the
    A/A pair (positions 2 and 3) is adjacent too.
  - **False flags:** if per-pass ratios behave like the A/A ratios under no
    change, a page exceeds the A/A p95 fold in the slow direction about 2.5% of
    the time per pass. Both passes together give about 0.06%, or roughly 0.1
    false flags per mode across 208 pages.
  - **Sensitivity:** the noise-floor flag is therefore conservative. A real 5–7%
    regression on a noisy page can go unflagged. The A/A pairs also cross the
    pass-1/pass-2 mode order, which can widen the floor further. The separate
    ">5% in both passes" list is the more sensitive view.
  - **Labeling:** both lists are labeled descriptive, which is correct. "No flag"
    must not be reported as "no regression", and an aggregate ratio must not be
    reported as per-page improvement.

## 5. New findings

### N1 — Blocker (process): the runtime is no longer frozen

**Where:** `rust/src/markdown.rs` (00:53:51, `967da51e`) and
`rust/tests/markdown.rs` (00:56:32, `1a92643d`). New fixtures
`rust/tests/data/markdown/retained-math-*.html` date from 00:50.

**Effect:** the prebuilt executable, the smoke evidence, the integrated corpus
gate, and `frozenSourceHashes` do not describe this tree. A final run started
now would rebuild the binary at run time, which defeats pre-building. It would
also time a renderer that has not passed the gates.

**Performance scan of the new code:** linear. There is one `ENTITY.find_iter`
pass per escaped text, one `block_containment` pass that builds two sets with
one visit per edge, and `retained_markdown` adds only a tag check. No
algorithmic hazard is visible. Its correctness is out of scope here.

**Fix:** see the section 6 checklist, items 1–4.

### N2 — Medium (evidence honesty): load average cannot show whether the machine was quiet

At rest, load average is 48.0/48.2/48.6 on 8 logical CPUs, while `vmstat`
reports 99% idle with 0 runnable tasks. 48 tasks sit in uninterruptible
sleep. 46 of them are blocked in `amdgpu_info_ioctl`: 37 `chromium`, 5
`chrome-headless`, and 4 `node`. The other two are kernel workers
(`amdgpu-reset-dev`, up for 15 days, and `ttm`). These tasks hold no CPU.

The smoke's `executionOrder` records about 49–51 for every worker. Load average
is the harness's only contention record (`benchmark.mjs:99,106`), so:
- a reader may wrongly conclude the runs were heavily loaded;
- real competing work of a few cores would be invisible within that ~48
  baseline.

The machine also has 9.6 GB of swap in use and about 1 GB free.

**Minimal fix:** state this in the final benchmark document. Record positive
idle evidence for the quiet window, either way:

- **Without a harness change:** before the run, check that no other agent, build,
  or test is active. Keep `vmstat 5` running into a file outside the output
  directory for the duration; it wakes once every five seconds, which is
  negligible. Accept the run only if user+system CPU stays near one core
  (≈12–15% of 8 CPUs) and `si`/`so` stay 0.
- **Optional harness change (outside timers):** sample `/proc/stat`
  (`cpu` busy jiffies, `procs_running`) and `/proc/vmstat`
  (`pswpin`/`pswpout`) in `runWorker` before and after each process, and add
  them to `executionOrder`. Any harness edit requires the pure checks,
  `--validate`, and the smoke again.

### N3 — Low (provenance): build freshness is not recorded

The claim that the release worker was pre-built and the final build is a no-op
is not recorded by the harness. Two minimal options:
- Store `fresh` from the selected `compiler-artifact` message next to
  `nativeBuiltExecutable` (`benchmark.mjs:84-86`).
- Without a harness edit: after the run, assert that `summary.executableSha256`
  equals the hash recorded at the refreeze, and that the repeat's
  `binaryHashes.baseline` is `98dfbb11…`.

`PATH`, and hence which `cargo` ran, is not recorded. Only `cargo --version` is.
This is acceptable.

### N4 — Low (correctness, outside the performance scope; refines the R1 section 3 edge case): hidden-retry candidates diverge from TypeScript

`pipeline.rs:207-217` selects the hidden retry root over all document
descendants, which includes `<html>` and `<body>`. It uses `max_by_key`, which
returns the **last** maximum. TypeScript's `findLargestHiddenContentSelector`
(`src/defuddle.ts:740-769`) instead:
- scans only `body.querySelectorAll`, so `<body>` itself is never a candidate;
- keeps the **first** strict maximum.

Probe with the existing release CLI (built at 00:38; this branch is unchanged
since) and the existing `dist/cli.js`. Anonymous inputs are in
`opus-perf-review-r2/probe-hidden/`:

| Input | TypeScript | Rust |
| --- | --- | --- |
| `body-hidden.html` (`<body class="hidden">`, 3-word `<main>`, 36-word sibling) | 3 words, `<main>` | 39 words, `<body>` content including text outside `main` |
| `nested-tie.html` (hidden `<section>` wrapping an equal-text hidden `<div>`) | root `<section>` | root `<div>` |

No corpus case exercises either input, since all 452 proofs are unchanged. The
minimal safe fix below reproduces the TypeScript candidate set and tie-break, and
counts words once per candidate:

```rust
let hidden = prepared.original.tree.body().and_then(|body| {
    let mut best: Option<(NodeRef<'_>, usize)> = None;
    for node in dom::select(body, crate::constants::HIDDEN_EXACT_SKIP_SELECTOR) {
        if crate::utils::attribute(node, "class").contains("math") {
            continue;
        }
        let words = count_words(&node.text());
        if best.is_none_or(|(_, best_words)| words > best_words) {
            best = Some((node, words));
        }
    }
    best.filter(|(_, words)| *words >= 30).map(|(node, _)| node)
});
```

Per `AGENTS.md`, this needs a minimal anonymous fixture that fails before and
passes after. The correctness owner should also confirm that `prepared.original`
has undergone the same shadow-root and streamed-content preparation that
TypeScript's clone does. If adopted, bundle it with the N1 refreeze. Otherwise
record it as a known deviation. **It is not a benchmark blocker.**

### N5 — Polish

The remaining 1.3 items (`words_at_most` in `patterns.rs:620-625` and
`:707-711`, regex-free site-name matching) are optional. Their costs are
captured by the final run, not hidden.

## 6. Go/no-go checklist for the root

**No-go now. Go when all of these hold:**

1. The renderer fix and its regressions are complete. The correctness review of
   that change is either re-run, or its interruption is documented with direct
   fail-before/pass-after evidence, as the status file plans. Decide whether to
   adopt N4 now or document it.
2. Refreeze: regenerate `frozenSourceHashes` for all 28 `rust/src` files and
   verify there are no unexpected differences.
3. Re-run the integrated gates on the frozen tree: the full native suite, the
   226-case semantic/security/readable-snapshot/prose gates, fmt, Clippy with
   `-D warnings`, 1.88 MSRV, and a release CLI smoke. Review every changed
   Markdown snapshot. The non-math corpus must still produce no warnings,
   because the native worker aborts on any warning.
4. Pre-build the release `--example benchmark` in the current-only target and
   record its SHA-256. Run a **new discarded smoke** (3 rounds, 1 warmup) with
   the native repeat, in a fresh cache directory. The previous smoke exercised
   `validate_output`, determinism, and cold/warm equality only for `b803a9ca`.
   Do not publish these timings.
5. Quiet window: stop or confirm idle every other agent, build, test, and
   browser workload. Record CPU-idle and swap evidence as in N2. Then run
   `node scripts/porting/benchmark.mjs rust/benchmarks/2026-10-21-hardening 7 2`,
   followed immediately by `benchmark-native-repeat.mjs` with the same output
   directory, the baseline binary, and the fork. Edit nothing in between.
6. After the run, assert that `summary.rustSourceHashes` equals the refrozen set
   and that `executableSha256` equals the item 4 hash. Confirm the repeat
   recorded baseline `98dfbb11…`. Check `vmstat` for contention and swap. If any
   page is flagged, or a previously flagged page crosses a threshold in only one
   pass, repeat the alternating sequence as the method report specifies.
7. Only then replace `rust/README.md:157-158` and `rust/BENCHMARKS.md`. Cite
   per-page results from the per-page lists, never from the aggregate. State the
   A/A flag's low sensitivity (section 4), the Vitest reference label, the
   GC/destruction asymmetry, the laptop CPU with the `schedutil` governor, and
   the N2 load-average note.

## 7. Remaining limits

- I ran no timing. Every speed statement belongs to the final run. The ratios in
  `hardening-run.json` logs and the smoke log are not evidence.
- The repeat's flags are descriptive. They are not significance tests, and
  neither they nor an aggregate prove the absence of individual regressions.
- The equivalence proofs cover default options with `markdown:true`, in default
  and debug modes. Other option combinations rely on the static proofs in
  section 3, which do not depend on options.
- The reference is TypeScript source under Vitest with linkedom, not
  `dist/node.js`. Rust DOM destruction is timed; V8 may collect garbage outside
  the timers.
- The machine is a laptop (i7-4870HQ, `schedutil`, turbo) with a hung GPU
  driver and heavy swap use. Results describe this environment only.
- I did not review the correctness of the in-flight `markdown.rs`, or of N4
  beyond the probe.

## Reproduction and readset

```bash
R=~/.cache/defuddle-port-hardening/opus-perf-review-r2
python3 -I $R/recompute_main.py ~/.cache/defuddle-port-hardening/benchmark-smoke-r2  # ALL_MATCH
python3 -I $R/recompute.py      ~/.cache/defuddle-port-hardening/benchmark-smoke-r2  # ALL_MATCH
T=~/.cache/defuddle-port-hardening/target/release/defuddle
TZ=UTC node dist/cli.js parse $R/probe-hidden/body-hidden.html --json
$T $R/probe-hidden/body-hidden.html --json
```

**Files read:**
- **Prior reviews and responses:** the R1 performance and correctness reports,
  the IMPROVEMENT_PLAN, the standardize, schema, benchmark, Markdown, and
  correctness-status responses, the method report, and `hardening-run.json`.
- **Harness:** all six benchmark scripts, the porting README, and
  `rust/examples/benchmark.rs`.
- **Runtime:** `standardize.rs`, `math.rs`, `removals.rs`, `pipeline.rs`,
  `dom.rs`, `dom/selectors.rs` (tripwire), `selection.rs` (`find_main`), and
  diffs of every runtime file against the R1 probe copy.
- **Dependencies and TypeScript:** dom_query `set_text`, and from
  `src/defuddle.ts` lines 120–160, 705–770, and 1005–1046, plus `math.ts` and
  `tsconfig.node.json`.
- **Evidence:** `perf-standardize-r2-*`, `schema-r2/verification.json` with its
  directories, and the smoke provenance, summaries, and raw files.
