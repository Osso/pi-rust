# Agent-core shell-output traceability

Pinned source: `packages/agent/src/harness/utils/shell-output.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

The current slice maps only exported `sanitizeBinaryOutput`. Direct differential proof invokes the real pinned agent-core module through the persistent agent-utility oracle. Eighteen valid-UTF-8 fixtures (`11000..=11017`) compare exact text, UTF-16 units, and code points; four source-only UTF-16 fixtures (`11018..=11021`) pin retained lone-surrogate behavior without constructing invalid Rust strings.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `shell-output.ts:30` | Export a pure string-to-string sanitizer | Public `sanitize_binary_output(&str) -> String`, focused ordinary/empty tests, and exact differential output comparison |
| `shell-output.ts:31` | Iterate with `Array.from`, preserving code-point order and supplementary pairs | Rust scalar iteration plus focused/direct supplementary, order, duplicate, UTF-16-unit, and code-point fixtures |
| `shell-output.ts:33-34` | Defensively reject an undefined code point | Typed-string exclusion: Rust iteration always yields a valid scalar; four direct UTF-16-unit fixtures separately pin the source's retained lone surrogates |
| `shell-output.ts:35` | Preserve tab, LF, and CR | Focused complete C0 range plus direct preserved-control and mixture fixtures |
| `shell-output.ts:36` | Remove every other value through U+001F | Focused generated range plus direct all-C0, boundary, mixture, and all-removed fixtures |
| `shell-output.ts:37` | Remove U+FFF9 through U+FFFB | Focused and direct U+FFF8..=U+FFFC boundary and mixture tests |
| `shell-output.ts:38-40` | Preserve every other code point and join without normalization | Focused and direct DEL/C1, whitespace/separator, format, BOM, bidi, private-use, unassigned, noncharacter, combining, variation, supplementary, ordering, duplicate, and all-removed fixtures |
| `shell-output.ts:121` | Captured stdout/stderr is sanitized, then CR is removed before accounting | Consumer mapping only; capture state and CR removal remain a later slice |

Source discrepancy: the source comment says lone surrogates are filtered by `Array.from`, but the implementation retains them. Rust `str` cannot represent lone surrogates, so this behavior remains an explicit typed-string exclusion rather than silently adopting the comment.
