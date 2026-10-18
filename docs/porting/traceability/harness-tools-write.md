# Write tool traceability

Pinned source: `packages/agent/src/harness/tools/write.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/write.rs`. Behavioral proof: `tests/harness_tools.rs` write tests. Differential: 3 write fixtures in `tests/differential_harness_tools/`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `write.ts:7-10` | TypeBox schema with required `path`/`content` and exact descriptions | `write_schema`; schema test and differential comparison |
| `write.ts:16-19` | Description text | `DESCRIPTION`; schema test |
| `write.ts:21-23` | Resolve path | `resolve_tool_path` |
| `write.ts:24-37` | Queue mutation; pre-abort check; write; post-abort check; UTF-16 byte-count success text | `execute_write`; success, pre-abort, and differential fixtures |

## Rust boundary

`content.encode_utf16().count()` reproduces JavaScript `content.length` for the success message. Details are absent (`None`), matching `details: undefined`.
