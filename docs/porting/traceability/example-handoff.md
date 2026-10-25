# handoff.ts — common-pin audit

Frozen `4f9e6f8a4`, 191 lines; native `examples/extensions/handoff.rs`. Opt-in native extension registration is the standing approved mapping.

| Symbols / branches | Native / proof |
| --- | --- |
| entryToMessage, getHandoffMessages | entry_message/handoff_messages; no-compaction and latest-compaction ordering, first-kept range and summary timestamp; actual-source pure-helper differential |
| /handoff mode/model/empty-goal/empty-history gates | run_handoff; `example_handoff` full runner differential |
| convertToLlm/serializeConversation, goal/system prompts | shared frozen-equivalent helpers; exact captured request/context in full runner fixture |
| loader/auth/complete/abort/error/text join | generate_draft/complete_draft; success, aborted, no auth, failures and loader input/frames in existing fixture |
| draft editor, empty/cancelled edit, newSession parent/withSession replacement UI | edit_draft_and_replace; old context invalidated and replacement editor/notification effects compared |

Audit found a scalar serde intermediate before selecting handoff entries. Raw message/summary units now stay in existing JsValue. `example_conversation_text` RED `/tmp/triage2-conversation-text-red3.log` rejected an escaped surrogate before comparison; repaired pure-helper differential covers both surrogate halves with/without compaction. The original complete-command fixture remains active. [Historical command matrices](extension-examples-i.md).

Gap: exotic wrongly typed inputs, OS/runtime faults and scheduling races follow the parity-bar exclusions; no new behavior or API carrier.
