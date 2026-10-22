# supervisor/service.ts

Frozen `4f9e6f8a4`; Rust `src/supervisor/service.rs`.

| Export / branch | Proof |
|---|---|
| `SupervisorEvaluator`, `RunSupervisorRequestInput` | Native cancellation-token/fallible future API; compiled consumers |
| `SUPERVISOR_INSTRUCTION_REJECTION_REASONS`, review decision/rejection types | Enum and exact ordered rejection array; `q16_new_exports` token matrix; `q16_veto_matrix` all five rejection outputs |
| `parseSupervisorInstructionReviewDecision` | Exact tokens, case/newline/non-string negatives: `q16_new_exports`; trim-token mutant killed |
| `supervisorInstructionRejectionFeedback` | All five exact hidden feedback messages: `q16_veto_matrix` |
| `buildSupervisorInstructionReviewContext` | Exact frozen context/system prompt/message/timestamp: `q16_new_exports` |
| `buildSupervisorPrompt` | Five request kinds: `differential_supervisor_service` |
| `parseSupervisorResponse` | Fifteen parse cases + empty-text/whitespace distinction: `differential_supervisor_service`, `q16_runtime_repin`; `q16_response_whitespace` compares six BOM/U+0085 cases against frozen ECMAScript trim |
| `runSupervisorRequest` | `supervisor`: completion, expiry, evaluator rejection, durable cancellation; `q16_veto_matrix` exercises frozen/Rust service completion through process handler |

Remaining: exhaustive cancellation/preemption interleaving differential and cooperative-evaluator rejection ordering. No whole-service completion promotion. [Slice proof](../q16-supervisor-runtime-proof.md).
