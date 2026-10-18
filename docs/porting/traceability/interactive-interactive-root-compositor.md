# interactive-root-compositor.ts

Source: `coding-agent/src/modes/interactive/interactive-root-compositor.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/interactive_root_compositor.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `InteractiveRootFlowLayout` alias | `InteractiveRootFlowLayout` | `interactive_workflows::interactive_root_compositor_preserves_order_and_reports_layouts` |
| `InteractiveRootCompositorOptions` | `InteractiveRootCompositorOptions` | same test |
| flow order header→loadedResources→chat→transcriptTail→pending | `RootCompositorOptions.flow` | same test (exact lines) |
| bottom order status→widgetAbove→editor→widgetBelow→footer | `RootCompositorOptions.bottom` | same test |
| transcript-tail flow callback | `with_on_flow_layout` | same test (rect/flowEndRow/bottomRow/bottomHeight/viewportHeight) |
| status/editor layout callbacks | `with_on_layout` | same test (editor row after height/footer/editor growth) |
| `getHeight` | `get_height` | same test (12 → 14) |

`RootCompositor` itself is ported and differentially tested in `pi-tui`.
