# Lossless orchestrator integration

Frozen source: `packages/orchestrator/src/**` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/**`.

## Native boundaries

JS paths, ids, labels, timestamps, argv/env values, logs and errors use `JsString`; environment names remain Rust `String`. JSON records and IPC/RPC envelopes use `JsValue`/`JsObject`, `parse_lossless`, `stringify_lossless` and native serde adapters. Public errors retain native messages; internal Radius errors additionally retain HTTP status for retry classification.

Shared authorities: `node_join` replaces the deleted local path joiner; `node_fs` supplies Node error messages; `header_value` performs the HTTP ByteString boundary before `pi_ai` Fetch dispatch. Native strings reach filesystem/socket/spawn or stdout/stderr only through `encode_node_utf8`. Original byte-ingress decoding stays at filesystem/pipe/socket boundaries, never projects an existing JS string to scalars.

## Coverage

| Module group | Exact behavior | Behavioral proof |
|---|---|---|
| config/types/storage | Native fields and paths, config precedence, Node path normalization, unknown-record preservation, stable instance order, JSON file bytes | foundation, differential, errors; shared `node_join` |
| ipc/protocol/client/server | Native request/response/error JSONL, number/key order, native parse errors, socket lifecycle, initial and stream frames | ipc, frame_whitespace, errors, verifier_regressions; approved coalesced-frame delta retained |
| supervisor/handler | Native instance keys, cwd/label/session metadata, spawn/recovery/persistence/list/status/stop and stream dispatch | supervisor, lossless_ipc |
| rpc_process | Native child request ids, responses/events/UI requests and error messages, correlation/disposal/EOF/UTF-8 boundary behavior | rpc_process, errors, frame_whitespace |
| radius | Native credentials, machine/Pi records, HTTP body/errors, ByteString headers, registration/disconnect/retry state | radius, radius_body; fake Fetch only |
| cli/serve | Native argv, cwd, output/log/error strings; JSONL forwarding and signal cleanup | cli, supervisor |

## Lossless regression

`lossless_ipc::lone_surrogate_cwd_label_and_id_survive_spawn_persist_list_over_real_ipc` covers `supervisor.ts:270-304`, `storage.ts:35-67` and `ipc/protocol.ts:129-140`. It observes the native cwd at the injected spawn callback, launches a real Node fake child in the OS-encoded U+FFFD directory, then checks original unpaired cwd/label/id units in spawn response, persisted instances.json, and list response over real Unix IPC. Stop also resolves the unpaired instance id and removes persistence.

Regression was committed before production migration. Its first invocation was blocked by 34 pre-existing library native-API compile errors; no behavioral RED execution is claimed. After migration it executes GREEN.

## Signal-fixture synchronization

The frozen `serve.ts:36` listening log precedes synchronous SIGTERM-handler registration at lines 59-61. Immediate signal delivery after reading stdout produced exit 143 in the existing test. The fixture now awaits a real IPC list round-trip before SIGTERM, proving setup returned to the event loop. Original exit-zero, socket-removal and log-equality assertions remain; no sleep/retry, frozen-source change or production workaround. The Rust half uses direct `shutdown()`, not a subprocess signal, so needs no symmetric signal-readiness barrier.

## Proof boundary

All existing fixture assertions are retained with native types; no ignored tests or compatibility shims. No live Radius/provider calls. Native OS ingress still follows existing UTF-8 decoding behavior. Detailed command/revision/result ledger and logs: `/tmp/pi-rust-repin-lossless-orchestrator/REPORT.md`.
