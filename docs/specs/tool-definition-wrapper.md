# Tool definition wrapper

Reference: frozen `packages/coding-agent/src/core/tools/tool-definition-wrapper.ts` at `4f9e6f8a4`.

- Preserve definition metadata, argument preparation and optional execution preparation. Absent preparation stays absent and never calls the context factory.
- Omitted context factories produce no context. A present factory supplies the base context; execution adds the current execution identity without mutating that base.
- Cap combined text output with the shared tail truncator unless details already contain `truncation` or `fullOutputPath`. Preserve non-truncated results unchanged.
- Each invocation shares one spill file between updates and final output, never across invocations. Spill full Node-encoded UTF-8 text to an eight-random-byte filename; retain exact UTF-16 in returned content and metadata. Spread array details as indexed properties.
- Partial-update spill failures surface synchronously and can be caught by the definition. Never defer them and override a recovered result. Uncaught asynchronous event/timer callback failures use the existing process-level uncaught handler; synchronous execution failures remain fallible results.

Rust-native registered extensions receive a context from their runner. Missing context there is outside the typed native boundary and returns an explicit error, never a fabricated context or panic. Standalone built-in factories preserve frozen missing-context behavior.

Proof map: [wrapper traceability](../porting/traceability/tool-definition-wrapper.md).
