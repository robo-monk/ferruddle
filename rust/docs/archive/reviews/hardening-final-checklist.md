# Final response to the second performance review

The Opus 5.5 xhigh report initially rejected measurement because the renderer
was changing. The coordinating agent completed the required work before the
two final measured runs. This response records completion evidence; it is not
an additional Opus sign-off.

| Finding or prerequisite | Final evidence |
| --- | --- |
| N1: runtime changed after freeze | All 28 runtime files were frozen again after the renderer and hidden-retry changes. The same hashes appear in both final summaries and `hardening-run.json`. |
| Finish renderer correctness work | 242 Rust tests pass; 40 original marker probes preserve literal text without activation. The independent peer report covers 2,082 cases through two Markdown parsers and 54 exact-source assertions. |
| Recheck corpus and changed Markdown | All 226 quality gates pass. No further snapshot or exception changed. Three Markdown spellings changed with identical rendered semantics; HTML and all other extraction fields stayed identical. See `hardening-r3-quality.json`. |
| Recheck compiler and tooling | Strict Clippy, rustfmt, Rust 1.88 library/CLI checks, release build and CLI smoke passed. Logs are retained in `validation/`. |
| N4: hidden candidate scope and ties | Fixed to inspect body descendants and retain the first maximum. Two anonymous fixtures fail before and pass after. A separate peer verified six TypeScript-matching probes and 452 unchanged complete corpus outputs. |
| Pre-build and discard a fresh smoke run | Final release worker `03f2927b…` passed the paired 208-case warm/cold smoke and four native repeats before measurement. Original/current builds used separate Cargo targets. |
| N2: monitor idle and swap | Both final runs retain five-second vmstat and before/after CPU, swap and blocked-task records. Active paging occurred. The benchmark report states this limit and makes no isolated-host claim. |
| N3: pre-build identity | Copied executables were checked around every worker. Final source and binary hashes equal the pre-build records in both host-monitor reports. |
| Final paired measurement and native repeat | Two complete runs, each with two warmups and seven rounds per mode/pass, followed immediately by original/current/current/original native measurement. Raw timings and output proofs are retained. |
| Independent arithmetic | Reviewer-authored Python programs recomputed every statistic and flag in both runs, with `ALL_MATCH`. |
| Update documentation after measurement | `README.md`, `BENCHMARKS.md`, `QUALITY.md` and final verification records describe the final source and both runs. |

The optional bounded counts in two remaining pattern climbs and a regex-free
site-name matcher remain future experiments. They are not required for this
port. Math conversion remains excluded, with the exact unsupported warning.

The provider interrupted the second Opus correctness review. Its partial
finding was fixed and independently checked as described above. That work does
not constitute a completed second Opus correctness review. See
[the interruption record](hardening-opus-correctness-r2-status.md).

See [final verification](../verification.json),
[benchmark evidence](../BENCHMARKS.md), and
[validation logs](validation/manifest.json).
