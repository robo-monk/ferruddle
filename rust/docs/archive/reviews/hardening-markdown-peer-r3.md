# Independent Markdown peer review, round 3

Independent GPT peer review, 2026-10-21. **No blocker found in the reviewed
Markdown change.** This is a bounded source and probe review; it is not Opus
review or a completion of the provider-interrupted correctness round 2.

Reviewed frozen `rust/src/markdown.rs` SHA-256:
`967da51e33499e4065d6081886cf98d60825caee64d335bdb4154514e7dec61d`.
The runtime remained unchanged throughout this review. Only this report was
written inside the repository.

## Read set and isolation

Read the first correctness review, the Markdown round-2 response, the interrupted
round-2 status, and the final round-3 Markdown response. Reviewed `markdown.rs`
in full, including every retained-HTML call site, escaping, inline merging,
list/table handling, and the two containment caches. Also inspected the math
source predicate, unconditional sanitizer, URL-resolution entry path, and the
retained-source fixtures and regressions. Compared CommonMark block tags with
the installed primary parser source, pulldown-cmark 0.13.4 `src/scanners.rs`.

The probe called only public `extract` and `html_to_markdown`, linking the actual
root crate at the frozen Markdown source. Its separate Cargo target was
`~/.cache/defuddle-port-hardening/target-markdown-peer-r3`; there were no baseline
builds or current-target writes. All source hashes at probe-build time are saved
in `markdown-peer-r3/source-before.json`. A later recheck found only root's
concurrent `pipeline.rs` N4 correction changed. That correction is outside this
review snapshot and is left to root's final integrated validation.

## Findings and evidence

**Blocking findings: none.** The general boundary fix covers the failure family
rather than only the originally reported tags:

- The block-tag matcher equals all 62 CommonMark type-6 tags plus the four
  type-1 starts. An independent set comparison found no missing or extra tags.
  Those roots receive blank-line boundaries. Type-7 inline void roots receive
  a static closing span wrapper; retained nonvoid inline roots close on the same
  physical line because retained text and attribute line breaks are encoded.
- All retained paths use the same structural serializer and boundary helper:
  math sources, sup/sub/SVG, media fallback, unusual lists, and complex tables.
  Invalid tag/attribute names cannot desynchronize a quote scanner because no
  such scanner remains. Page-derived text, punctuation, attributes, and line
  breaks are escaped where emitted.
- Literal `<` in normal text becomes an entity. Strict HTTP/email candidates
  produce explicit Markdown links, so neither valid autolinks nor weak email-like
  comments, processing instructions, declarations, or CDATA expose a raw opening
  delimiter. Inline-node splitting cannot reconstruct one from page text.
- Separate prose and Markdown containment caches preserve inline math holders
  with img/br or inert scripts while separating following Markdown blocks. The
  caches are built bottom-up once; no subtree scan was introduced per renderer
  node.

Independent probes exercised **2,082 cases**, rendered by both pulldown-cmark
0.13.4 and Marked 18.0.14, with JSDOM 24.1.3 DOM readback:

| Check | Result |
| --- | --- |
| Original 40 source-hash-verified reviewer inputs/options | Prior root evidence: 28 marker activations; current: zero |
| Literal marker activation, both renderers | Zero |
| Unsafe URL, event-handler, or srcdoc attributes | Zero |
| Following emphasis/strong, heading, and list-item count drift | Zero |
| Exact retained-source text and attribute readback | 54 assertions passed |
| Duplicate or changed unsupported-math warnings | Zero |

The generated cases cover 84 tag variants, all type-1/type-6 starts, inline void
roots and hard breaks, same-line/blockquote/list contexts, every split position
of three literal marker strings, weak and strict autolink candidates, quote-bearing
attribute names, invalid tag names, all retained-output families, and poisoned
URLs in default, debug, standalone, and disabled-cleanup modes. The original
40-case replay uses the exact options stored in root's before evidence, including
`standardize: false` without changing its other options.

Structure assertions compare with final extracted HTML for extraction cases and
source DOM for standalone conversion. This avoids classifying earlier metadata
or cleanup removals as Markdown structure defects. Exact source checks cover
retained pre/textarea leading newlines, blank lines, tabs, literal entity
spellings, Unicode, nested inert script converted to code, MathML/annotation,
complex-table pre/code, and mixed img/br holder attributes. Safe HTTP and email
links, including slash/equal email local parts, still render as links; malformed
domains remain literal. The warning remains `math_not_supported` with the exact
message `Math support is not yet included.` Math conversion remains excluded.

## Existing limits and optional follow-up

No optional finding below is introduced by the reviewed boundary fix:

- Nonempty iframe raw text does not decode character references on HTML
  reparse. This accepted limitation is already documented in both Markdown
  implementation responses.
- Earlier extraction normalization may remove source-holder attributes or
  normalize code whitespace before the renderer receives the DOM. A standalone
  inert script rendered as an inline code span follows the existing newline-to-
  space convention; a script inside a retained source holder preserves exact
  parsed text. Exact retained-source assertions deliberately distinguish these
  paths. The code-fence path also has its established boundary-newline convention.
- Serializing an extracted pre/textarea with an initial LF and then parsing that
  returned HTML independently can consume the LF again. Retained Markdown
  correctly preserves the original parsed LF. This existing HTML-serialization
  behavior is outside the boundary change.
- The separate image-caption normalization case reported by the Markdown worker
  can drop a source-figure attribute before Markdown retention. It remains a
  separate source-preservation follow-up, not a new raw-HTML activation path.

No full suite, MSRV run, corpus gate, benchmark, snapshot update, allowlist edit,
or runtime modification was performed here. Root owns those final checks. A
finite probe matrix cannot prove safety for every Markdown consumer, but both
independent downstream renderers agree on the tested failure families.

## Reproduction artifacts

All probe inputs, source/input/environment hashes, raw outputs, scripts, and the
machine-readable conclusion are under
`~/.cache/defuddle-port-hardening/markdown-peer-r3/`. The definitive files are
`verification.json`, `jobs.json`, `results-expanded.json`, and
`cross-parser-expanded-summary.json`; the latter records all assertion failures
as empty arrays. `verification.json` hashes the probe source, lockfile, binary,
source read set, jobs, raw outputs, and analysis script. The source of the first
40 cases is unchanged under `opus-review-r2/inputs-inert/`; the prior observation
is root's `root-r2-before.json`.

To replay the frozen probe binary and analyze its Markdown without rebuilding:

```sh
cache="$HOME/.cache/defuddle-port-hardening"
"$cache/target-markdown-peer-r3/debug/defuddle-markdown-peer-r3" \
  "$cache/markdown-peer-r3/results-expanded.json" \
  < "$cache/markdown-peer-r3/jobs.json"
node "$cache/markdown-peer-r3/cross-parser.mjs"
```

Rebuilding later links the later runtime and therefore constitutes a new
snapshot; use the working toolchain environment and the dedicated target above.
