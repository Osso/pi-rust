# Change working directory tool

Match `packages/coding-agent/src/core/tools/change-working-directory.ts` at frozen `4f9e6f8a4`.

- Sequential tool; reject child runtimes before target validation. A nonempty agent ID or required-agent-ID flag identifies a child.
- Require exactly one nonempty path or ID. Preserve path whitespace; trim ID whitespace. Explicit null and non-string fields fail the source validation; they are not treated as absent.
- Resolve a direct path relative to context cwd. For an ID, use the shared session lookup, reject the current session, then read its recorded cwd without modifying that session.
- Require an existing directory: existence check, then a separate real stat, then directory classification. Stat failures retain native Node fields in the helper and propagate their exact message through the existing message-only ToolError boundary. Preserve original UTF-16 values in diagnostics, relocation, result content and details; encode only at the filesystem boundary.
- Schedule relocation with the current tool-call ID. Return terminating text and `previousCwd`, `cwd`, `source`, optional `sessionId` details. Actual session transition remains runtime-owned.
- Call renderer selects path, ID or `target`. Result renderer joins text blocks only and styles nonempty output by result error status. Reuse the prior Text component; incompatible retained components throw the frozen native TypeError rather than being replaced.

Native contexts always provide the relocation method, as does frozen runner.ts:519-525. Missing-method custom JavaScript contexts are excluded from the typed API, accepted 2026-10-24.

[Traceability](../porting/traceability/change-working-directory.md) records proofs and remaining scope.
