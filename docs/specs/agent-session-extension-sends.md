# Agent-session extension sends

Frozen authority: `agent-session.ts:5334–5353,3907–3952` at `4f9e6f8a4`.

- [x] A custom send without a triggered turn commits its synchronous state/persistence effects before the extension API returns.
- [x] Pending user/custom sends continue asynchronously; rejected sends emit an ExtensionError with extensionPath `<runtime>` and event `send_user_message` or `send_message`.
- [x] Error text follows frozen Error.message/String handling. Stack omission retains the repository approval.

The two existing send adapters share the existing inline-poll dispatch and one private error-reporting helper. No new public API, carrier type, executor or dependency is introduced. The next-turn path remains synchronous.

Proof: native registered-extension tests assert immediate message/storage counts and an actual before-run rejection reaching the runtime error listener. See the [proof ledger](../porting/agent-session-repin-proof.md).
