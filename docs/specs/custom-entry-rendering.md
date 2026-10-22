# Custom entry rendering

Frozen contract: `4f9e6f8a4` custom-entry.ts, render-region-container.ts and interactive-root-compositor.ts. Proof: [entry lifecycle](../porting/traceability/interactive-custom-entry.md), [chat regions](../porting/traceability/render-region-container.md), [root layouts](../porting/traceability/interactive-interactive-root-compositor.md).

- Entry renderers receive expansion, session ID, scoped request-render and identity-deduplicated cleanup registration. Every generation owns its callbacks, even when returning no component or throwing.
- Rebuild invokes the replacement renderer before retiring prior callbacks and component. Invalidation invalidates existing children first. Cleanup precedes optional disposal; repeated explicit disposal is inert.
- Entry renderer throws produce the frozen padded error box, preserving exact UTF-16 message units and theme styling. Empty output has no spacer/content and is disposed when not mounted. Custom-message renderer throws instead use that component's default Markdown box. Both catches remain local with the process crash handler installed.
- Chat repaints only tracked, mounted children with known parent/child layout; failed partial repaint returns false without scheduling a root render. Removing/clearing scoped entries retires regions and components, including tracked-but-unmounted children.
- Root chat layout is required; widget-above layout is optional. Both preserve frozen root-entry order and geometry. Generic extension TUI request routing is unchanged.
