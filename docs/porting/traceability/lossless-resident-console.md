# Lossless resident console transport

Frozen source: `core/resident-console-transport.ts` at `4f9e6f8a4`.

Snapshot cwd/session IDs, host prompt text/IDs/errors, client pending IDs/disconnect errors and socket paths use `JsString`. Both wire directions parse with `js_json::parse_lossless`; generic branches/events deserialize through native `JsValue`. Queued outgoing messages remain `JsValue` until socket-writer serialization. Filesystem/socket path encoding occurs only at OS calls.

`tests/lossless_resident_console.rs::socket_prompt_preserves_unpaired_units_and_correlates_raw_id`: actual socket RED at `/tmp/pi-rust-repin-lossless-tools/console-red.log` rejects a raw surrogate prompt as invalid; intended response accepts and correlates the unchanged raw ID. Harness includes the actual module and actual `utils/private_paths.rs`, no DB replacement. GREEN at `/tmp/pi-rust-repin-lossless-tools/console-green.log` (1/1). Additional client regression covers native snapshot fields/branch JSON, event JSON, rejected prompt errors, and raw socket-path encoding. Existing transport behavior tests are adapted to the native host contract.

Architect host signature updated only; architect snapshot/queue/path body migration remains main-owned. Supervisor caller is migrated next. Final combined socket suite and mutants remain pending.
