# coordination-visibility.ts and display queue previews

Frozen `4f9e6f8a4` `coding-agent/src/modes/interactive/coordination-visibility.ts:1–25` fully compared. Native `src/modes/interactive/coordination_visibility.rs`.

| Frozen branch | Native | Proof |
|---|---|---|
| mailbox regex :4–5, optional host/session and nonempty agent | native legacy UTF-16 JsRegex | dedicated differential: bare, hosted/session, session-only, missing host hint, empty agent, CRLF and wrong prefix; raw-surrogate native case |
| `isCoordinationPreview` :7–12 | typed `QueuedMessagePreview` | shared-channel always classified; envelope only with extension provenance; differential12 preview variants |
| `isCoordinationMessage` :14–25 | `AgentMessage` match and exact joined text blocks | differential7 message variants including split text/image, interactive/no source, custom/wrong custom role |

## Source queue boundary

Parent authorized narrow `agent-session.ts` slice (2026-10-22), not a whole-module claim:
- :469–473 `QueuedMessagePreview`: native text/input_source/custom_type record at its frozen home, no JsValue carrier.
- :853–855 queue storage: one preview record per pending display entry; Agent still owns delivery.
- :3414–3448 enqueue: text and input source retained; shared-channel :3736 retains custom type.
- :1499–1529 remove-on-start: compare preview.text, preserve source first-match removal.
- :3999–4048 clear/text getters/snapshot getter: text APIs remain text-only; snapshots clone records without draining queues.
- interactive-mode.ts :5530–5550 filters session previews only when visibleOnly and hideCoordinationMessages; compaction queue remains visible and is appended by mode.
- :5569–5585 pending rendering uses TruncatedText, source labels and dequeue hint; removed duplicated old-only Follow-up label.

## RED proof

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:
- `q13-queue-previews-red.log`: original display text projection loses extension provenance while retaining raw text.
- `q13-queue-ui-red.log`: real streaming session exposes hidden extension coordination in pending rows.

Post-commit `15f0eca7`: `q13-previews-coordination_visibility-green.log`2/2 (12 preview +7 message variants and raw-envelope case); `q13-queue-ui-green.log`1/1; `q13-previews-agent_session-green.log`52/53, only known mailbox RED (`delivered mailbox prompt`). Extended UI compaction/visibleOnly assertions have a subsequent focused proof log. Tests use a gated real Faux stream, not source-shape assertions. They assert snapshot mutation independence, non-consumption, text clearing, provenance, raw units and real UI visibility while delivery remains queued. Whole agent_session target is required; known runtime-mailbox failure remains main-owned and must be reported, not relaxed.
