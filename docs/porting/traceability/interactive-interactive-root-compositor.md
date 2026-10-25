# interactive-root-compositor.ts

Source: `coding-agent/src/modes/interactive/interactive-root-compositor.ts`
Rust: `crates/pi-coding-agent-interactive/src/modes/interactive/interactive_root_compositor.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `InteractiveRootFlowLayout` alias | `InteractiveRootFlowLayout` | `interactive_workflows::interactive_root_compositor_preserves_order_and_reports_layouts` |
| `InteractiveRootCompositorOptions` | `InteractiveRootCompositorOptions` | same test |
| flow order header→loadedResources→chat→transcriptTail→pending | `RootCompositorOptions.flow` | same test (exact lines) |
| bottom order status→widgetAbove→editor→widgetBelow→footer | `RootCompositorOptions.bottom` | same test |
| transcript-tail flow callback | `with_on_flow_layout` | same test (rect/flowEndRow/bottomRow/bottomHeight/viewportHeight) |
| status/editor layout callbacks | `with_on_layout` | same test (editor row after height/footer/editor growth) |
| `getHeight` | `get_height` | same test (12 → 14) |

## Common-pin revalidation (2026-10-22)

Whole module compared with frozen `4f9e6f8a4`, lines 1–42. Required chat layout (10,29) and optional widget-above layout (20,35) now forward to root entries; status/editor and flow-tail callbacks preserved.

`differential_interactive_root::root_flow_and_bottom_callbacks_match_frozen_typescript` compares exact frames and all callback payloads/order at heights 12,14,8,4, both with and without the optional widget callback (eight layouts). Initial RED omitted chat callback; GREEN 1/1 in `/tmp/pi-rust-repin-lossless-interactive/test-scoped-entry-green-4.log`. `interactive_workflows` retains editor/footer growth coverage.

`RootCompositor` itself is ported and differentially tested in `pi-tui`.
