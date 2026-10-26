# Historical Mermaid renderer

The current common TypeScript pin does not render Mermaid. The user approved porting the historical feature from `532f7d12`, a separate lineage locked in `delta-lock.json`. Its `grok-mermaid` 0.2.2 JavaScript oracle and the vendored Apache-2.0 Rust renderer provenance are pinned separately; see [oracle gate](../oracle-gate.md) and [vendor provenance](../vendor/grok-mermaid-PROVENANCE.md).

## Status

The renderer core and historical `markdown.mermaid` setting are implemented, but interactive Markdown is not yet wired to the renderer. Mermaid blocks still render as code in the application. Do not classify the feature as complete until the interactive process and frozen reference tests pass.

| Historical behavior | Rust status and proof |
| --- | --- |
| Flowchart, sequence, state, class and ER rendering | `pi-tui/src/mermaid.rs`; focused differential compares normal output, width, styles and Unicode graphemes with the pinned JavaScript package. |
| Flowchart warnings and strict-parser final-line retry | Focused warning, retry and oversize differentials pass; interactive warning presentation remains pending. |
| `markdown.mermaid` off/final/streaming settings | Settings and selector behavior with bidirectional historical settings fixtures; interactive mode is not yet wired. |
| Streaming/final mode, width fallback, theme spans and thinking exclusion | Pending interactive integration and behavioral proof. |

Non-scalar UTF-16 Mermaid input remains a recorded exotic-input gap. Integration must retain the original fenced block without replacing text when scalar rendering is not possible.
