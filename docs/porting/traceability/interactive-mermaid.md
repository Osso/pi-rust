# Historical Mermaid renderer

The common TypeScript pin does not render Mermaid. The user approved porting the historical feature from `532f7d12`, a separate lineage locked in `delta-lock.json`. Its `grok-mermaid` 0.2.2 JavaScript oracle and the vendored Apache-2.0 Rust renderer provenance are pinned separately; see [oracle gate](../oracle-gate.md) and [vendor provenance](../vendor/grok-mermaid-PROVENANCE.md).

## Status

The renderer, settings and interactive Markdown integration are implemented. Focused tests cover the paths below; a whole-module audit and final workspace verification remain pending.

| Historical behavior | Rust status and proof |
| --- | --- |
| Flowchart, sequence, state, class and ER rendering | `pi-tui/src/mermaid.rs`; focused differentials compare normal output, width, styles and Unicode graphemes with the pinned JavaScript package. |
| Flowchart warnings and strict-parser final-line retry | Focused warning, retry and oversize differentials. |
| `markdown.mermaid` off/final/streaming settings | Settings and selector behavior with bidirectional historical settings fixtures. |
| Streaming/final mode, width fallback, theme spans and thinking exclusion | Markdown and message-component tests plus an interactive PTY test: assistant output becomes Unicode art while thinking remains fenced code. |

Non-scalar UTF-16 Mermaid input remains a recorded exotic-input gap. The renderer leaves the original fenced block intact when scalar rendering is not possible.
