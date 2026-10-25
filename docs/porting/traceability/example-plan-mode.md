# plan-mode/index.ts — common-pin audit

Frozen: `packages/coding-agent/examples/extensions/plan-mode/index.ts`, `4f9e6f8a4`. Native: `crates/pi-coding-agent/examples/extensions/plan_mode/index.rs`.

| Symbols / branches | Native mapping | Current proof |
| --- | --- | --- |
| default factory, plan flag, /plan, /todos, Ctrl+Alt+P | plan_mode_extension, toggle | exact registration, empty/progress notifications and repeated toggles |
| tool-name uniqueness, saved tool set, planning disable/restore | unique, enable, normal | custom tools retained; plan tools added once; original tools restored |
| tool_call, context, before_agent_start | registered handlers, assistant_text, remaining | safe/blocked/other tools; stale string/block/custom contexts; exact plan/execution prompts |
| turn_end, agent_end, updateStatus | finish_turn, update | partial/full completion, stay/execute/refine/cancel, trimmed follow-up, widget/status bytes |
| persistState, session_start restoration | persist, restore | flag override, latest state, execution-marker cutoff, owned entry snapshots |
| generated split-surrogate todo text | existing JsString/JsValue through all above paths | exact source/native UI traces, execution/completion messages, restore and JSONL escape assertions |

`example_plan_mode` has three passing tests. The raw-text test starts with valid astral text that frozen truncates between surrogate units. RED `/tmp/triage2-plan-lossless-red2.log` observed native replacement in persisted JSONL. Native text now stays lossless in notifications, widgets, messages, state and restored entries. `/tmp/triage2-example_plan_mode-final-dev.log` proves all three tests after the repair. The oracle uses the actual frozen runner's readonly session context; the obsolete copied-helper override and toWellFormed projection are removed. Owned append snapshots follow the standing native ownership approval.

Gap: wrong-typed/exotic inputs and OS/runtime faults are outside the user-observable parity bar. No semantic deviation, new dependency, API carrier, or TS delta was added. Historical replacement/projection claims in [batch L](extension-examples-l.md) are superseded here.
