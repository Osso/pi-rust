# Lossless tools fixture sweep

Frozen source: common pin in [source-lock](../source-lock.json). Native fixtures retain `JsString`/`JsValue`; generated OS paths require strict UTF-8. Runtime proofs use sandboxed Cargo tests only. This ledger is bounded fixture evidence, not whole-module acceptance.

| Fixture scope | Revision | Proof | Remaining gap |
|---|---|---|---|
| `control_command`, `repin_control_{agents,exports,lifecycle,mailbox,migrations,raw_json,reciprocal,requests,retention_clock,sessions,validation,whitespace}` | `f6c128b8` | `cargo test -j4 -p pi-coding-agent` selecting these 13 targets: 71 PASS, zero warnings; `fixture-control-small-test.log` | None in selected assertions; broader sweep pending |
| `differential_control_db`, `differential_multi_agent_store` | `b6107565` | Targeted Cargo test: 4 PASS, zero warnings; `fixture-control-diffs-test.log` | Incoming ASCII restriction removed by the dedicated native incoming slice below. |
| `lossless_incoming_content` | RED `81c32624`; production `8194e3e5`, fixture follow-up `6759cfaa` | Actual sandboxed Cargo RED: native DTO ingress rejects lone units (`unexpected end of hex escape`); `incoming-content-incrate-red.log`. GREEN58 across incoming content/error, control command, control differential and session-control DB; `incoming-content-incrate-green-2.log`, zero warnings. | Producer accepts native content, DTO/claim return native content, and SQLite binding alone repairs unmatched units. |

## Further bounded fixture groups

| Scope | Revision | Cargo proof / unresolved result |
|---|---|---|
| `host_relay_resident`, `repin_runtime_mailbox_transcript`, `lossless_runtime_mailbox`, `terminal_outbox_delivery`, `differential_agent_lifecycle_trace`, `js_value_artifacts` | `d0ba36e6` | GREEN26, zero warnings; `fixture-relay-small-test.log` |
| `approved_mailbox_deltas` | `1972b7c1` | GREEN4; fixed stale creation timestamp (expired/missing refs), not production behavior. `fixture-approved-mailbox-green.log` |
| `host_relay_store`, `host_relay_protocol`, `cross_implementation_control_db` | `ba4b36a2` | GREEN18, zero warnings; `fixture-relay-large-test.log` |
| `bash_stdin`, `bash_tool`, `bash_tool_detach`, `differential_bash_tool`, `tool_definition_wrapper` | `cab2513c` | GREEN30, zero warnings; `fixture-bash-group-test.log` |
| `differential_bwrap` | `ec198dfe` | GREEN4; fixture environment maps wrapped as native JSON objects; `hook-native-incrate-green.log` |
| `claude_bash_hook_extension` | RED `4fe83515`, production `29bce163` | GREEN8, including raw real-process input keys/output reason; `hook-native-incrate-green.log` |
| `differential_claude_bash_hook` | `ee2437ad` | GREEN1, zero warnings; `hook-differential-green.log` |
| `detached_pyrun_evaluation` | `f824f07b` with shared support `51489afc` | Previously blocked; now GREEN13 real runner/lifecycle cases, zero warnings; `hook-native-incrate-green.log` |

## Session and question fixture completion

| Scope | Revision | Cargo proof |
|---|---|---|
| `change_working_directory_tool`, `resume_session_tool`, `search_current_session_history_tool`, `session_coordination_tools`, `session_tool_output` | `347978c0` | GREEN51, zero warnings; `fixture-session-small-test.log`. Persisted byte-cap assertions still measure encoded bytes. |
| `agent_session_runtime_mailbox`, `ask_questions_tool`, `differential_session_tools` | `1350065f` | GREEN26, zero warnings; `fixture-session-final-test.log` |
| `question_relay` | `a5577a64` | GREEN9, zero warnings; `fixture-question-relay-green.log`. Error assertions inspect native text, not Debug output. |

Final inventory: original 119 lane files minus 31 reassigned files; 85 owned test/support files. Every owned inventory file changed since the sweep baseline `41971f15`. This is migration coverage, not fresh whole-workspace acceptance. Parent owns centralized checks, independent verification and any parent-owned `agent_session` regressions. Final strict-path scan found no `to_string_lossy`; one residual native relay ID assertion is made exact without UTF-8 repair. `lossless_tool_paths` lacked saved in-crate execution evidence and receives a focused Cargo run; no redundant broad rerun is required.

Internal serialized process identities remain scalar by the existing exception; fixture parsers decode their JSON losslessly before typed deserialization. Scalar provider/model/role/thinking tags are asserted at their existing typed API boundary. Human content and arbitrary argument JSON have no scalar exception.
