# Agent-core shell-output traceability

Pinned source: `packages/agent/src/harness/utils/shell-output.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slice maps only exported `sanitizeBinaryOutput`. Focused Rust proof covers valid UTF-8 strings; direct TypeScript differential proof is pending for this production commit.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `shell-output.ts:30` | Export a pure string-to-string sanitizer | Public `sanitize_binary_output(&str) -> String` plus focused ordinary/empty tests |
| `shell-output.ts:31` | Iterate with `Array.from`, preserving code-point order and supplementary pairs | Rust scalar iteration plus focused supplementary, order, and duplicate fixtures |
| `shell-output.ts:33-34` | Defensively reject an undefined code point | Approved typed-string exclusion: Rust iteration always yields a valid scalar |
| `shell-output.ts:35` | Preserve tab, LF, and CR | Focused complete C0 range and mixture tests |
| `shell-output.ts:36` | Remove every other value through U+001F | Focused generated U+0000..=U+001F test and boundary fixtures |
| `shell-output.ts:37` | Remove U+FFF9 through U+FFFB | Focused U+FFF8..=U+FFFC boundary and mixture tests |
| `shell-output.ts:38-40` | Preserve every other code point and join without normalization | Focused DEL/C1, format, BOM, bidi, private-use, unassigned, noncharacter, combining, variation, supplementary, ordering, and all-removed fixtures |
| `shell-output.ts:121` | Captured stdout/stderr is sanitized, then CR is removed before accounting | Consumer mapping only; capture state and CR removal remain a later slice |

Source discrepancy: the source comment says lone surrogates are filtered by `Array.from`, but the implementation retains them. Rust `str` cannot represent lone surrogates, so this behavior remains an explicit typed-string exclusion rather than silently adopting the comment.
