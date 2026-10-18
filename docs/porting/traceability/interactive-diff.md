# diff.ts

Source: `coding-agent/src/modes/interactive/components/diff.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/diff.rs`
Shared diff: `crates/pi-agent-core/src/jsdiff.rs` (`diff_words`)

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `parseDiffLine` (match / no match) | `parse_diff_line` | differential cases 1-11, including `diff --git`, `---`, `+++`, `@@` non-matches |
| `replaceTabs` | `replace_tabs` | tab case |
| `renderIntraLineDiff` removed/added/keep parts, first-part leading whitespace | `render_intra_line_diff` | differential cases 1-16 |
| single removed+added pair → intra-line diff | `render_diff` | differential cases 1-16 |
| multiple removed/added → plain lines | `render_diff` | differential case 2 |
| standalone added line | `render_diff` | differential case 3 |
| context line | `render_diff` | differential cases 1-4 |
| trailing empty line from `split("\n")` | `render_diff` | differential case 11 (`""` input) |
| `RenderDiffOptions.filePath` (unused in source) | omitted | source ignores it |
| `Diff.diffWords` tokenizer (word/whitespace/punctuation stitching) | `jsdiff::tokenize_words` | differential cases 1-16 |
| `WordDiff.equals` (`trim()` comparison) | `jsdiff::tokens_equal` (`Mode::Words`) | differential cases 6-9 |
| `WordDiff.join` (strip leading whitespace from non-first tokens) | `jsdiff::join_tokens` (`Mode::Words`) | differential cases 6-9 |
| `postProcess` whitespace dedupe (insert/delete/both, start/end keeps, overlap) | `jsdiff::dedupe_change_object_whitespace` | differential cases 6-9 |
| line diff unchanged | `jsdiff::diff_lines` (`Mode::Lines`) | existing edit differential suites |

Adaptations: `Diff.diffWords` is called with default options only (no `ignoreCase`, `oneChangePerToken`, `intlSegmenter`, `maxEditLength`, `timeout`); the Rust port implements exactly that path. The tokenizer uses the JavaScript `\s` set rather than Unicode `White_Space` so that word boundaries match the source.

Gaps: none for the source's reachable behavior. The unreachable jsdiff option paths are intentionally not ported.
