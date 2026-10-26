# supervisor/client.ts

Pin: [source-lock](../source-lock.json). Contract: [spec](../../specs/supervisor-client.md).

| Export / branch | Rust | Proof |
|---|---|---|
| RequestSupervisorDecisionInput; SupervisorClientDependencies; constants | `supervisor/client.rs` | Whole native typed input; numeric fields f64; optional startup seam |
| requestSupervisorDecision; attempts, retry, timeout suffix | same | Existing durable-response/retry tests; actual-frozen `differential_supervisor_client`14 scenarios |
| pre-abort; startup throw; abort during startup; source ordering of throw before second abort guard | same | Actual-frozen differential |
| abort listener, fixed reason, durable sender-fenced cancellation, long poll and retry interruption | same + shared AbortSignal/control DB | `supervisor_client::abort_cancels_the_durable_request_before_a_long_poll_finishes`; differential durable status/response comparison |
| date deadline, fraction/zero/negative attempts, NaN/Infinity timeout, Node timer normalization | shared date/timers | Actual-frozen differential; native RangeError constructor retained |
| completed and externally cancelled polling | same | completed durable response/retry tests; `supervisor_client_cancellation` proves claimed cancellation clears ownership and external error reason is returned |

The control DB retains raw lossless response JSON ([F01 reciprocal proof](session-control-db.md#supervisor-response-json-preservation-f01-2026-10-26)). Completed/cancelled polling projects `SupervisorResponse` only for typed client consumers; projection errors propagate as `NativeNodeError`, and no projection writes back to SQLite.

Interface-only signal509cf723 is now consumed. Production startup uses the whole coordinator; its late-exec limitation remains [pending user decision](supervisor-ensure-running.md), not silently treated as client parity.
