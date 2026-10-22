# Lossless orchestrator integration

Frozen source: `packages/orchestrator/src/**` at the [common pin](../source-lock.json).

| Boundary | Required behavior | Proof |
|---|---|---|
| `supervisor.ts:270-304`, `storage.ts:35-67`, `ipc/protocol.ts:129-140` | Native instance id, cwd and label retain every UTF-16 unit through spawn, persistence and IPC list; only the OS cwd boundary encodes unpaired units | `crates/pi-orchestrator/tests/lossless_ipc.rs`: regression added before implementation; baseline library fails to compile against native node_fs APIs, so behavioral execution is blocked until migration |

Active scope: native JS string/JSON authorities throughout `pi-orchestrator` source and fixtures; no frozen-source edits or unrelated crate behavior changes. Verification: zero-error/warning `cargo check -p pi-orchestrator --all-targets`, then `cargo test -j4 -p pi-orchestrator`.

Foundation implementation: config paths use shared `node_join`; record fields and IPC envelopes use native `JsString`/`JsValue`; storage reads/writes lossless JSON, retaining unknown fields and UTF-16 parse-error messages. Full-crate verification pending consumer migration.

Lifecycle/transport implementation: supervisor identity maps and callbacks retain native strings; IPC/RPC JSONL parses losslessly and encodes only at socket/stdio writes. Native paths reach OS APIs only through `encode_node_utf8`. Radius keeps credential/error/body strings native, using shared `header_value` for HTTP ByteString headers and lossless JSON HTTP bodies. Byte decoding stays at original filesystem/pipe/socket ingress boundaries, not on JS strings.

`lossless_ipc` is GREEN: native unpaired cwd/label/id survive the injected spawn boundary, real Node fake child, stored instances.json, list and stop over actual Unix sockets. Development proof: `/tmp/pi-rust-repin-lossless-orchestrator/lossless-ipc-development-02.log`.

Existing fixture migration: all integration targets now use native JSON/string expectations and explicit UTF-8 test-path construction; all original lifecycle, file-byte, CLI, malformed-input, stream and frozen-TS differential assertions remain. All-target compilation reached zero errors; compiler-proven unused imports removed. Full crate execution pending.
