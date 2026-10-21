# Second correctness review: interrupted

The Opus 5.5 xhigh provider interrupted this delegated review after flagging
the session. It offered to switch automatically to Opus 4.8 and change future
fallback behavior. The coordinating agent selected **Stay on Opus 5.5**, in
line with the user's explicit model request. No model setting was changed,
and no final review report or sign-off was produced.

The partial review verified all 28 frozen runtime hashes and built isolated
probes under `~/.cache/defuddle-port-hardening/opus-review-r2`. It found that
some retained math-holder tags can start a CommonMark HTML block, causing
following page text to be interpreted as HTML. Inert marker probes cover
image holders before breaks, `menu`, `search`, `legend`, `nav`, `textarea`,
and iframe roots. The coordinating agent completed a general renderer fix.
The original 40-case replay changed from 28 marker activations to zero, with
literal text preserved in every case. The separate
[GPT-6.1-Sol xhigh peer review](hardening-markdown-peer-r3.md) checked 2,082
cases through two Markdown parsers plus 54 exact-source assertions. The
[final verification](../verification.json) covers the integrated source.

This is a partial finding, not a completed independent correctness review.
The completed first correctness review remains available in
`hardening-opus-correctness-r1.md`. The second performance review is a
separate task and does not replace the interrupted correctness review.
