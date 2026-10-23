# Literal done/error stream events

Frozen `4f9e6f8a4` custom-provider-anthropic maps unknown stop tags to `error`, then emits a `done` event carrying that reason. This TypeScript nominal-type violation is a classified source bug, preserved by parent approval on 2026-10-23.

Native streams must retain that event kind, reason, final message, usage and absence of a fabricated errorMessage. Completion resolves normally with the carried message; downstream agent/print/RPC handling follows its actual stopReason. Proxy reconstruction forwards the same done reason and sets the reconstructed message stopReason to error.

Other provider producers keep their frozen terminal-event decisions. This exception does not authorize additional done reasons or converting ordinary error events to done.
