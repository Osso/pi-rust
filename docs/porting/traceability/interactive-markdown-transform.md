# markdown-transform.ts

Source: `coding-agent/src/modes/interactive/components/markdown-transform.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/markdown_transform.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `MarkdownTransformContext` | `MarkdownTransformContext`, `MarkdownMessageType` | differential (`markdownTransform`) |
| `MarkdownTransformer` | `MarkdownTransformer` (`Rc<dyn Fn(..) -> Option<String>>`) | differential transformer chain |
| `createMarkdownTransform` closure | `create_markdown_transform` / `MarkdownTransformFn` | differential inputs |
| transformer returns string | `Some(value)` replaces markdown | differential `upper`, `append` |
| transformer returns void | `None` keeps markdown | differential `drop` |
| transformer throws | `None` keeps markdown (caught in TS) | differential `throw` |

Adaptation: a throwing transformer and a `void` transformer are observationally identical in the source (both keep the current markdown), so the Rust callback returns `Option<String>` for both. The context/transformer types are local until the extensions slice publishes its shared types.
