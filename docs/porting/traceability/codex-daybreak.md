# Codex Daybreak traceability

Source: `packages/coding-agent/extensions/codex-daybreak/src/index.ts` at common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/codex_daybreak.rs`.

| Frozen symbol / branches | Rust | Proof |
|---|---|---|
| factory, command description, handlers, per-extension state | `codex_daybreak_extension` and startup registry | `suite_extensions::codex_daybreak::codex_daybreak_matches_frozen_handlers_and_wire_payloads` |
| object classification, three providers, four Blue model ids | `apply_request`, `codex_fast::supports_fast_mode`, `applies_to` | frozen differential supported base/gc/team and unsupported provider/model cases |
| child id (including empty), requires-agent-id gate | `is_child_runtime` | child/empty/required cases |
| status off/active/inactive and selection info/warning | `update_status`, `notify_selection` | command, session-start and model-select outputs |
| persisted latest recognized blue/null, skip wrong type/customType/invalid data | `read_persisted_program` | restore-blue with unrelated and invalid later entries; explicit-null override |
| empty query, trim/case, blue/off/usage, provider refusal, editor clear | `apply_command`, registered wrapper | differential command vectors |
| request guards; nonobject warning before model eligibility; request model rather than selected model | `apply_request` | malformed payloads, restricted selected model with eligible request model |
| access_programs merge/replace with exact arbitrary values | `apply_request` | existing object, array replacement, missing object, raw UTF-16 property values |
| persistence object `{cyber: daybreak_blue or null}` | `apply_command` | exact entry data/order in all mutating command cases |

## Proof ledger

- RED: `/tmp/bundled-daybreak-red.log`: missing first-party registration after invoking the real frozen oracle.
- Tests use private in-memory session branches and no provider calls. The oracle loads unmodified frozen source.
- Gap: pathological input shapes/runtime races/OS faults follow the shared user-observable parity bar; ordinary object payloads remain exact UTF-16 and insertion ordered.
