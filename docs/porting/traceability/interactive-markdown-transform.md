# markdown-transform.ts — removed old-only helper

Verified: 2026-10-22.

Frozen common pin: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([source lock](../source-lock.json)).

`git ls-tree -r 4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d -- packages/coding-agent/src/modes/interactive/components/markdown-transform.ts` returns no entry. The source module is absent, not an unported frozen capability.

| Former item | Disposition | Proof |
|---|---|---|
| `MarkdownTransformFn`, `create_markdown_transform`, transformer application helper | Rust module and export removed | Frozen path absent; parent re-pin removal decision 2026-10-22 |
| Differential `markdownTransform`, case id 3 | Removed with its retired helper, not relaxed | Before removal Rust produced transformed text while oracle could not load the absent frozen module |

No parity claim remains for this old-only helper. Existing extension transformer types and unrelated component APIs are outside this bounded removal.
