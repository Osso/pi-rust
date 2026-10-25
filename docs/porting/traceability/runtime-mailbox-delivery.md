# runtime-mailbox-delivery.ts

Whole55-line current-pin audit2026-10-25 against frozen [4f9e6f8a4](../source-lock.json). Native consumer implementation is in `crates/pi-coding-agent-core/src/core/agent_session.rs`; no extra public callback API was introduced for proof.

| Frozen export/branch | Native mapping and proof |
|---|---|
| `createRuntimeMailboxUserMessage`: text block then images, input source, current timestamp | `runtime_mailbox_user_message`; user-role/content/inputSource construction audited; real prompt/steer use, reciprocal User JSONL and existing image/prompt tests |
| `deliverClaimedRuntimeMailboxMessages`: ordered intercept and immediate acceptance | `accept_intercepted_mailbox_messages`; actual source helper/native AgentSession all-handled and mixed two-message cases observe durable first receipt before second interceptor |
| no remaining prompt messages →false | all-handled case has two receipts, no user prompt, no claimed rows left |
| formatted unhandled batch joined by two newlines | mixed case compares the complete frozen prompt with actual faux user input; intercepted body is absent |
| steer before durable acceptance →true | native steering test persists actual queued user message once, updates store and completes lifecycle; source order/return audited literally |
| prompt awaited, accepted transformed message or custom marker →false | idle prompt/tool turn integration and reciprocal marker/user acceptance; full native drain reenters at tool-result checkpoint without deadlock |
| onAccepted follows durable acceptance for every batch element | native `accept_runtime_mailbox_messages` invokes store projection/detached completion only after acceptance; source ordering audit plus before-next-interceptor receipt observation |
| finally releases all original claims, retains accepted receipts; release rejection overrides prior result | native `deliver_ready_runtime_mailbox_messages` stores awaited Result, executes release with `?`, then returns prior Result; unpersisted/invalid-role retry tests and distinct accepted/unaccepted batch tests |

`agent_session_runtime_mailbox`11/11 PASS atd7c587b52 (`/tmp/storage3-mailbox-delivery-green.log`), including new source/native interceptor2 scenarios and existing prompt/steering/tool/finalizer boundaries. Source callback-order fixture executes the real frozen module with a real private manager/JSONL and caller-supplied messages; it is not canonical-row ownership proof. Separate [acceptance reciprocal fixtures](runtime-mailbox-acceptance.md) prove real producer/claim/delete ownership in both directions.

The 82-line [acceptance module](runtime-mailbox-acceptance.md), complete-record [receipt scanner](runtime-mailbox-transcript.md) and canonical CAS functions remain their own authorities. Whole AgentSession/control-store acceptance is not claimed. No production correction or new carrier/API was needed in this audit. Protected strict fixture types and Biome pass; warning-mode scoped Clippy has zero changed-line diagnostics. Manual changed-line readability review completed; numerical metrics unavailable.

Gap: adversarial typed inputs, exceptional OS/fsync faults and runtime races follow current parity exclusions; no new fault-carrier or scheduling emulation.
