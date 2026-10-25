# RPC client

The client follows frozen `packages/coding-agent/src/modes/rpc/rpc-client.ts` and the declared protocol in `rpc-types.ts` at the [common source pin](../porting/source-lock.json). Branch proofs are mapped in [traceability](../porting/traceability/rpc-client.md).

## What it must do

- [x] Spawn the configured executable (default `node`) with the frozen CLI argument order, environment, startup checks, stop escalation and pending-request errors. A failed asynchronous spawn retains a process identity until stop; another start must reject, and stop then permits a fresh attempt. Concurrent stops share one termination attempt/result. Cancelling an already-polled waiter does not cancel the owned stop operation; caller-cancelled public SDK results follow the approved native cancellation boundary. ESRCH is a non-emitted false kill result; other child errors consume the frozen once-error observer, and a repeated unhandled error reaches the shared native uncaught authority. Captured stderr remains available when a closed/unrecognized host descriptor discards its mirror; FILE/stream write errors and default fatal exit statuses match Node.
- [x] Preserve parsed external JSON envelopes, property presence, raw UTF-16 and unknown properties through correlation and event delivery. Queue request lines in FIFO order without awaiting OS stdin backpressure; asynchronous stdin errors use Node text and reject every pending request. Hold every option string as JsString until Node's OS UTF-8 conversion; validate raw file/argv/cwd/env strings and preserve synchronous native constructor/code fields without retaining a child.
- [x] Preserve complete UTF-16 error messages and native own fields through `RpcClientError::message/value`; `Display` performs only Node's actual OS UTF-8 encoding.
- [x] Apply frozen success truthiness and Error construction before decoding a declared getter result; distinguish missing data from explicit null.
- [x] Settle wait/collection on the first `agent_end`, regardless of `willRetry`; subscribe before prompting. Remove terminal subscriptions during notification and retain the live listener-array iteration semantics. The internal collection promise expires while prompt acknowledgment is pending and survives a prompt rejection; an unobserved timeout uses the shared process authority. Explicit public-future cancellation retains the approved native SDK boundary.
- [x] Accept binary64 millisecond timeouts and normalize them through the shared Node timer authority. Callback failures use `Result<(), CallbackError>`; frozen's encompassing line-dispatch catch swallows them literally, without uncaught routing.
- [x] Expose only the frozen declared commands and return types; retain unknown properties and order in declared model reference/info records.

## How it works

- [Client and protocol traceability](../porting/traceability/rpc-client.md)
- [Protocol types traceability](../porting/traceability/rpc-types.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/rpc/rpc_client.rs`: private child, correlation, events and typed getters.
- `crates/pi-coding-agent/src/modes/rpc/rpc_types.rs`: native declared wire records; the old-only available-thinking-level query is excluded.
- `crates/pi-coding-agent/src/modes/rpc/rpc_client_models.rs`: declared ModelReference/ModelInfo fields plus raw ordered extras.
- `crates/pi-ai/src/types/model_properties.rs`: shared raw-property/order authority; decode/serialize visibility only is approved for client records.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_rpc_client.rs`: actual frozen client and native client against identical private stdio fixtures.
- `crates/pi-coding-agent/tests/rpc_client.rs`: native process/lifecycle and command behavior.
- `crates/pi-coding-agent/tests/rpc_types.rs`: declared DTO wire contracts.

## Known gaps (current cycle)

- Approved typed return boundary: external values that cannot inhabit a declared getter type reject explicitly, including foreign-server extras no frozen server emits. Successful decoding preserves the parsed value on reserialization. Actual frozen-server getter responses are proved separately; per-getter rejected fields are enumerated in traceability.
- Other accepted native runtime/language boundaries are listed once in [client traceability](../porting/traceability/rpc-client.md#discrepancies-and-approved-runtimelanguage-boundaries).

## Out of scope

- Live providers, host state, and unapproved semantic deviations.
- Accepted [shared child envp ordering gap](../porting/traceability/child-process.md#open-shared-spawn-item): native kernel environment ordering differs, without a new backend or test exclusion.
- Injected EINVAL/ENOSYS for fixed TERM/KILL returns the native Result error without preserving Node's orphan timer; the codes are unreachable on supported platforms absent fault injection (main ruling2026-10-24).
