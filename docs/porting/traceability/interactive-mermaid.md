# components/mermaid.ts

Source: `coding-agent/src/modes/interactive/components/mermaid.ts`
Rust: none.

Status: gap (user decision 2026-10-18).

`mermaid.ts` delegates diagram parsing and layout to the third-party
`grok-mermaid` package (~632 KB, ten modules under `node_modules/grok-mermaid`).
That library has no Rust port and is not a workspace dependency, so the
transformer is not ported. Mermaid fenced code blocks therefore render as ordinary
Markdown code blocks.

## Exported symbols

| TypeScript export | Rust item | Status |
|---|---|---|
| `createMermaidMarkdownTransformer` | none | gap |

## Gaps

- `createMermaidMarkdownTransformer`, `codeSpan`, `styleSpan`, `themedLines`,
  `isMermaid`, and all mode/width/warning branches are unported because they depend
  on `grok-mermaid` output (`MermaidArt`, `Span`, `render`).
- `MermaidRenderingMode` handling (`off`, `streaming`, `on`) is therefore absent;
  callers pass no mermaid transformer.

## Discrepancies

None (intentional, approved gap).
