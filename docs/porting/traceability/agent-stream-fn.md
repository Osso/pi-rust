# Traceability: `packages/agent/src/stream-fn.ts`

Rust: `crates/pi-agent-core/src/agent/stream_fn.rs`. Tests: `crates/pi-agent-core/tests/stream_fn.rs`.

| TypeScript export | Rust item | Tests |
|---|---|---|
| `setDefaultStreamFn(streamFn \| undefined)` | `agent::set_default_stream_fn(Option<StreamFn>)` | `default_stream_fn_is_process_global_and_clearable` |
| `getDefaultStreamFn()` | `agent::get_default_stream_fn() -> Result<StreamFn, AgentError>` | `default_stream_fn_errors_with_exact_message_when_unset`, `default_stream_fn_is_process_global_and_clearable` |

Branches:

- Unset default: returns `AgentError` with the exact TS message `No default stream function configured. Pass streamFn explicitly or call setDefaultStreamFn().`
- Process-global storage: `OnceLock<RwLock<Option<StreamFn>>>`; tests serialize access because the value is shared process state.
- TS throwing getter maps to `Result` (typed error) instead of a panic.
