# Agent-session headless startup and reload

Frozen authority: `agent-session.ts:280–298,980–986,5192–5197,5584–5588,5617–5629` at `4f9e6f8a4`.

- [x] The configured headless auto-detach delay is parsed as a positive safe integer using JavaScript number syntax. Standalone runtimes do not create a detach registry.
- [x] When a release path is configured, binding writes `<path>.ready` with exact bytes `ready` and waits for `<path>` before emitting session_start.
- [x] Reload copies existing flag values into the new extension runtime before runner construction, retaining their insertion order.

An absent environment option keeps the source default; an empty release path is an explicit error. Existing numeric-extreme and OS-fault exclusions remain applicable. No new carrier type, public API or dependency is introduced; reload uses the existing ordered FlagValue map instead of an unused JSON projection.

Proof: three native RED/GREEN tests cover real child-process release/ready ordering, a configured 20ms auto-detach, and flags surviving a fresh runtime. Two ignored child probes are explicitly executed by the parent tests. See the [proof ledger](../porting/agent-session-repin-proof.md).
