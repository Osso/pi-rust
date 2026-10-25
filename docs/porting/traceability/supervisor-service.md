# Supervisor service

Whole frozen module read: `packages/coding-agent/src/supervisor/service.ts` at `4f9e6f8a4`. Rust: `crates/pi-coding-agent/src/supervisor/service.rs`. [Contract](../../specs/supervisor-service.md).

| Frozen export / branch | Rust / proof |
| --- | --- |
| `SupervisorEvaluator`, `RunSupervisorRequestInput` | Native fallible evaluator, shared AbortSignal, durable request input. |
| Instruction reason/decision types, five reasons | Native enums; serialized values and fixed reason order compared with frozen. |
| `parseSupervisorInstructionReviewDecision` | Exact six-token table; all tokens plus padded/case/empty/non-string inputs. |
| `supervisorInstructionRejectionFeedback` | Five exact feedback strings. |
| `buildSupervisorInstructionReviewContext` | Stateless system prompt, one native user message, supplied fractional timestamp; raw UTF-16 differential. |
| `goalRequirementAuthorityContract`, `goalProgressResponseContract`, `responseContractForRequest` | Exact common-pin text; existing five-kind prompt differential. |
| `buildSupervisorPrompt` | Native text plus exact pretty request JSON; existing prompt differential. |
| `parseSupervisorResponse` and object/content/JSON helpers | Existing scalar/native parse differentials; full branch audit in progress. |
| `runSupervisorRequest`, `waitForEvaluation` and claim/deadline helpers | Cancellation and awaited-abort lifecycle re-pin still open. |

Baseline RED reproduced at 26b5de0e5: `/tmp/runtime-services-red-supervisor-prompts.log` (pre-repin text vs cumulative-objective frozen text). New instruction exports RED E0432: `/tmp/runtime-services-red-supervisor-review-exports.log`. Prompt/helper GREEN pending at this implementation commit. The whole-module coverage row is not yet done.
