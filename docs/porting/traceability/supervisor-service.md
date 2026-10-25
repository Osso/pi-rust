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

Baseline RED reproduced at 26b5de0e5: `/tmp/runtime-services-red-supervisor-prompts.log` (pre-repin text vs cumulative-objective frozen text). New instruction exports RED E0432: `/tmp/runtime-services-red-supervisor-review-exports.log`. Prompt/helper GREEN at 992a263c8: all three service differential tests pass (`/tmp/runtime-services-green-supervisor-prompts.log`). The whole-module coverage row is not yet done.

## Durable evaluator lifecycle re-pin

Six actual-source/real-SQLite RED cases at 992a263c8 (`/tmp/runtime-services-red-supervisor-runtime.log`): evaluator rejection incorrectly completes the request; pending cancellation never settles; settled cancellation attempts stale-claim completion; settled deadline incorrectly approves; settled lower-priority work bypasses approval preemption; abort drops evaluator cleanup before requeue. Native now polls first with the frozen 300ms default, applies cancellation/deadline/preemption in frozen order, awaits abort cleanup without JoinHandle::abort, checks cancellation around the final evaluator await, and lets evaluation errors propagate. Cancelled is an explicit outcome. Source response parse/complete behavior is otherwise unchanged. GREEN at 78be0daab: six exact runtime cases pass (`/tmp/runtime-services-green-supervisor-runtime.log`).

First GREEN batch at 46b481b11 had four passes and two 500ms wall-clock timeouts under concurrent source-process load. Instrumented focused cancellation runs proved cancelled status, signal emission and awaited completion (both pass; `/tmp/runtime-services-debug-supervisor-cancel.log`). Temporary diagnostics were removed. These six fixtures now use each private Tokio runtime's paused clock; the 500ms liveness assertion and all exact source state/trace assertions are unchanged, so host CPU delay cannot consume the evaluator timer budget. Production clock/state behavior is not changed by the fixture.

Empty-text RED `/tmp/runtime-services-red-supervisor-empty-text.log`: frozen first-empty-text extraction falls back to the original response object; native tried to parse an empty JSON string and discarded a valid approval. Native now uses the same nonempty-text condition, without looking at later text blocks. Nonempty invalid text still fails rather than falling back. Parser GREEN pending at commit.
