# Historical Mermaid renderer

The common TypeScript pin does not render Mermaid. The user approved porting the historical feature from `532f7d12`, a separate lineage locked in `delta-lock.json`. Its `grok-mermaid` 0.2.2 JavaScript oracle and the vendored Apache-2.0 Rust renderer provenance are pinned separately; see [oracle gate](../oracle-gate.md) and [vendor provenance](../vendor/grok-mermaid-PROVENANCE.md).

## Status

The renderer, settings and interactive Markdown integration are implemented through `1045d5fa0`. The table identifies focused test coverage, not complete parity proof. A whole-module audit and final workspace verification remain pending.

| Historical behavior | Rust status and proof |
| --- | --- |
| Flowchart, sequence, state, class and ER rendering | `pi-tui/src/mermaid.rs`; focused differentials compare normal output, width, styles and Unicode graphemes with the pinned JavaScript package. |
| Flowchart warnings and strict-parser final-line retry | Focused warning and retry differentials; native oversize test. |
| `markdown.mermaid` off/final/streaming settings | Settings and selector behavior with bidirectional historical settings fixtures. |
| Streaming/final mode, width fallback, theme spans and thinking exclusion | Markdown and message-component tests plus an interactive PTY test: assistant output becomes Unicode art while thinking remains fenced code. |
| Terminal controls in source | Controls are stripped before parsing; one ANSI-colored flowchart label is compared with the historical JavaScript oracle. Broader control-sequence parity remains unverified. |

Unsupported, unrenderable or too-wide diagrams retain the original fenced block. In final mode, parser warnings retain the block and add a warning; streaming mode renders available art. Non-scalar UTF-16 Mermaid input remains a recorded exotic-input gap and retains the fenced block.
