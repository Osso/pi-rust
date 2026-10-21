# supervisor/main.ts

Frozen `4f9e6f8a4`; Rust `src/supervisor/main.rs`.

| Export / branch | Proof |
|---|---|
| `GENERIC_GOAL_CONTINUATION`, instruction reviewer/evaluator types | `q16_veto_matrix`; fallible native closure aliases |
| `SupervisorConsolePromptQueue`, request loop | `supervisor`, durable DB/request-loop differential (blocked by upstream transcript entry; not filtered) |
| tool allow/exclude lists, settings, extension validation, KB path gate | `supervisor` behavior tests; frozen literals manually compared |
| `createSupervisorResourceLoader` | Old-only remote compaction removed; KB gate tested; no live discovery |
| `createSupervisorConsoleSnapshot` | Stable identity/readiness/current PID/version behavior; resident wire differential preserves identity; config version dependency assertion remains RED |
| `openSupervisorSession` | `q16_resident_open`: exact frozen selected filename and surviving files; approved resident delta unchanged |
| `createSupervisorInstructionReviewer` | Fake-provider evaluator, exact token matrix; pre-abort bypass |
| `createSupervisorInstructionEvaluator` | Native ModelRuntime creation facade; injectable runtime helper tested using faux provider only; quota provider-selection failures not exhaustive |
| `processSupervisorRequest` | `q16_veto_matrix` nine-case full frozen feedback/response comparison; stale boundary/error behavioral tests; cancellation during compaction RED/GREEN |
| `cleanupSupervisorProviderContext` | Compiled provider cleanup wiring; no live provider |
| service/session creation (private TS helpers) | Authenticated gpt-6.1-sol provider order; low reasoning, response tool and tool set behavioral tests |
| `runSupervisorService` | Compiled console/wake/session/evaluator/signal wiring only; deliberately not started live |

No whole-module promotion: raw transcript/config dependencies and unexercised live orchestration remain. [Slice proof](../q16-supervisor-runtime-proof.md).
