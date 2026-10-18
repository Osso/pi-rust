# markdown-transform.ts

Source: `coding-agent/src/modes/interactive/components/markdown-transform.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/markdown_transform.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `MarkdownTransformContext` | `core::extensions::types::MarkdownTransformContext` | differential (`markdownTransform`) |
| `MarkdownTransformer` | `core::extensions::types::MarkdownTransformer` (`Arc<dyn Fn(..) -> String>`) | differential transformer chain |
| `createMarkdownTransform` closure | `create_markdown_transform` / `MarkdownTransformFn` | differential inputs |
| transformer returns string | replaces markdown | differential `upper`, `append` |

Gap: the source tolerates a transformer returning `void` or throwing (it keeps the current markdown). The shared `MarkdownTransformer` type returns `String`, so those runtime-only cases are not representable and are excluded from the differential.
