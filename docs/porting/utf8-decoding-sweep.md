# File UTF-8 decoding sweep

Verified: 2026-10-26. Base: `ededb0e7762ce91c8a7efe175bb8df0a5d54799b`.
Implementation tip: `5073e4860` (subsequent audit-document commit changes no Rust).

Scope: production file reads under `crates/**`. Match the frozen Node UTF-8 file decoder with `String::from_utf8_lossy`; preserve genuine I/O errors. No network/provider calls, new dependencies, subprocess protocol redesign, merge or push. The frozen core is `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`; native Claude extension source is `38d84cb549cce66dd76d45a5020d132f74780393` under the read-only extension snapshot.

## Fixed sites

Native paths below are relative to `crates/`. Frozen paths are relative to `packages/coding-agent/`, except AI dependency and extension paths explicitly identified.

| Native production site | Frozen counterpart | Result / native test |
|---|---|---|
| `pi-coding-agent/src/architect/project_spec.rs`, `read_project_spec` | `src/architect/project-spec.ts:30`, `readFile(canonicalPath, "utf8")` | Fixed; `project_spec_replaces_invalid_utf8_in_content_with_valid_index` |
| Same file, `find_project_root` | `project-spec.ts:37`, `readFile(index, "utf8")` | Fixed readability probe; `project_spec_replaces_invalid_utf8_in_index_and_content` |
| `pi-coding-agent-base/src/utils/changelog.rs`, `read_changelog_file` | `src/utils/changelog.ts:122`, `readFileSync(..., "utf-8")` | Fixed; `changelog_replaces_invalid_utf8_without_dropping_sections` |
| `pi-coding-agent-extensions/src/bundled_extensions/bwrap/worker.rs`, `read_grep_lines` | `extensions/bwrap/src/fs-worker.cjs:182`, `readFile(..., "utf8")` | Fixed; `grep_lines_replace_invalid_utf8` |
| Same file, `readText` operation | `fs-worker.cjs:285`, `readFile(..., "utf8")` | Fixed; `read_text_replaces_invalid_utf8` |
| `pi-ai/src/api/google_client.rs`, `load_credentials` | Frozen dependency `google-auth-library/build/src/auth/googleauth.js:545-553`, `createReadStream` followed by `setEncoding("utf8")` | Fixed; `adc_file_replaces_invalid_utf8` (offline ADC only) |
| `pi-ai/src/api/bedrock_credentials.rs`, `read_sso_token` | Frozen dependency `@smithy/core/dist-cjs/submodules/config/index.js:183`, SSO `readFile(..., "utf8")` | Fixed; `sso_token_replaces_invalid_utf8` (isolated child HOME, no request) |
| Same file, `parse_ini` | Same dependency `config/index.js:270`, shared-INI `readFile(path, "utf8")` | Fixed; `shared_ini_replaces_invalid_utf8` |
| `pi-coding-agent-extensions/src/native_user_extensions/claude_bridge/account_profiles.rs`, `read_credentials` | Extension `src/account-profiles.ts:74`, `readFileSync(path, "utf8")` | Fixed parse and decoded copy bytes; `credentials_replace_invalid_utf8_in_values_and_copy_bytes` |
| Same file, `freshest` active marker | `account-profiles.ts:123`, `readFileSync(marker, "utf8")` | Fixed; `freshest_ignores_a_nonmatching_invalid_utf8_marker` |
| Same file, `read_active_profile` | Extension `src/index.ts:159`, `readFileSync(..., "utf8")` | Fixed; `active_profile_replaces_invalid_utf8` |
| `claude_bridge/auth_trace.rs`, descriptor credential read | Extension `src/auth-trace.ts:98`, `readFileSync(descriptor, "utf8")` | Fixed; `credential_trace_replaces_invalid_utf8` |
| Same file, `profile_files` active marker | `auth-trace.ts:112`, `readFileSync(..., "utf8")` | Fixed; `profile_trace_replaces_invalid_utf8_marker` |
| `claude_bridge/sessions.rs`, `SessionStore::recorded` | Extension `src/native-prefix.ts:258`, `readFileSync(recordPath(...), "utf8")` | Fixed; `association_replaces_invalid_utf8` |
| `claude_bridge/session_records.rs`, `read_records` | Frozen `cc-session-io` `parseJsonlFile`, `readFileSync(path, "utf-8")` | Fixed; `session_records_replace_invalid_utf8_bidirectionally`, including unmodified frozen reader on invalid input and native-written JSONL |
| `claude_bridge/session_verify.rs`, `verify_written_session` | Extension `src/session-verify.ts:18`, `readFileSync(jsonlPath, "utf8")` | Fixed; `verification_replaces_invalid_utf8` |
| `pi-coding-agent-interactive/src/modes/interactive/interactive_mode.rs`, small `read_agent_log_preview_unchecked` | `src/modes/interactive/interactive-mode.ts:3504`, `readFileSync(logPath, "utf8")` | Fixed; `agent_log_preview_replaces_invalid_utf8` |
| `pi-coding-agent-extensions/src/bundled_extensions/pyrun/detached_evaluation.rs`, `read_complete_artifact_records` | `extensions/pyrun/src/detached-evaluation.ts:328`, `readFileSync(path, "utf8")` | Fixed; `complete_artifact_records_replace_invalid_utf8`; keeps complete following records and excludes the incomplete tail |

Every production change had a failing test first. Fixtures include an incomplete multibyte sequence followed by ASCII and an invalid byte, asserting maximal-subpart U+FFFD replacement and retained neighboring data. Marker-routing tests assert the frozen nonmatching-marker behavior instead of an encoding error. Genuine I/O error conversion branches retain their existing text.

## Strict file reads retained

Parent confirmed the activation/checksummed-manifest exclusion on 2026-10-26: these files are machine-written UTF-8; invalid bytes require corruption and fall under the OS/data-fault gap. The same category applies to the relay process-identity record. This does **not** claim TS rejects invalid bytes at these boundaries.

| Native production site | Frozen counterpart | Reason left strict |
|---|---|---|
| `pi-coding-agent-tools/src/core/detached_bash_runner.rs:204`, activation | `src/core/detached-bash-runner.ts:75`, `readFileSync(path, "utf8")` | Internal machine-written activation identity; corruption gap, not user-authored data |
| Same file `:569`, launch manifest | `detached-bash-runner.ts:252`, `readFileSync(path, "utf8")` | Internal checksummed machine-written manifest; approved corruption exclusion |
| `pi-coding-agent-extensions/src/bundled_extensions/pyrun/detached_runner.rs:155`, activation | `extensions/pyrun/src/detached-runner.ts:366`, `readFileSync(path, "utf8")` | Internal machine-written activation identity; approved corruption exclusion |
| Same file `:164`, launch manifest | `detached-runner.ts:370`, `readFileSync(path, "utf8")` | Internal checksummed machine-written manifest; approved corruption exclusion |
| `pi-coding-agent-tools/src/core/host_relay_resident.rs:24` | `src/core/host-relay-resident.ts:27`, `readFileSync(...relay.json, "utf8")` | Internal machine-written process identity/version; corruption gap |
| `pi-coding-agent-storage/src/core/runtime_process.rs:55` | `src/core/runtime-process.ts:38`, `readFileSync(/proc/.../stat, "utf8")` | Kernel data; invalid process-name bytes are an OS/path-encoding edge, not a user-authored file |
| `pi-coding-agent-extensions/src/bundled_extensions/pyrun/runner.rs:647` | `extensions/pyrun/src/runner.ts:113`, `readFileSync(/proc/.../stat, "utf8")` | Same kernel-data boundary |
| `pi-ai/src/api/openai_codex_responses.rs:856` | `packages/ai/src/api/openai-codex-responses.ts:1636`, `os.release()` | Native `/proc/sys/kernel/osrelease` substitutes for OS API; kernel ASCII release, no TS text-file reader |
| `pi-coding-agent-base/src/utils/windows_self_update.rs:56` | `src/utils/windows-self-update.ts`, `process.report.getReport().sharedObjects` | Native `/proc/self/maps` substitutes for runtime API; native-addon/Windows gap, not user text-file input |

## Other strict decoding found, outside file-read scope

These read pipes, sockets, terminal input or binary protocols, not files. They remain unchanged; their inclusion here is not a parity closure claim.

| Native site | Frozen counterpart / classification |
|---|---|
| `pi-coding-agent-data/src/core/desktop_notification.rs:100`, `Read::read_to_string` | `src/core/desktop-notification.ts:50`, notify-send stdout `setEncoding("utf8")`; subprocess numeric notification ID |
| `pi-coding-agent/src/cli/login_command.rs:78`, stdin `read_line` | `src/cli/login-command.ts:56`, input `setEncoding("utf8")`; terminal/login input, not file input |
| `pi-coding-agent-tools/src/core/resident_console_transport.rs:280,515,559,729`, `lines` / `read_line` | `src/core/resident-console-transport.ts`, socket `chunk.toString("utf8")`; IPC, not file input |
| `pi-coding-agent-extensions/src/bundled_extensions/pyrun/runner.rs:517,533`, stdout/stderr `read_line` | `extensions/pyrun/src/runner.ts:249-250`, stdout/stderr `setEncoding("utf8")`; subprocess protocol/output, not persisted artifacts |
| `pi-coding-agent-extensions/src/native_user_extensions/claude_bridge/process.rs:293`, stdout `lines` | Extension SDK child stdout JSONL; subprocess protocol, not the bridge session-file reader |

Other `from_utf8` calls are not strict file readers: terminal/network incremental decoders validate prefixes and handle partial/error sequences; Bedrock binary eventstream/header decoding is a wire boundary; URL/percent decoding, ASCII numeric rendering and hex/key parsing operate on memory. No user-authored text-file path was found through these calls. Test-only reads, frozen-oracle output parsing, vendored regex implementation and string `.lines()` calls were excluded. `crates/**` examples/build sources were also searched; no additional strict file reads were found.

## Existing replacement-decoding file readers

No change was needed for settings/keybindings (base `ededb0e77`), `models.json`/auth storage, themes, prompts/templates, skills, AGENTS/rules, resource-loader text, package/extension metadata, ordinary session JSONL/history/archive, or pi-atlas cache/session parsing. Their production paths already read bytes and use replacement decoding. Representative sites: `settings_manager.rs:264`, `keybindings.rs:349`, `model_registry.rs:571`, `auth_storage.rs:183`, `prompt_templates.rs:204`, `skills.rs:189,298`, `resource_loader.rs:254`, `rule_store.rs:199,261`, `extensions/loader.rs:408`, `package_manager.rs:2002,2505,3696`, `session_manager.rs:1774`, `session_history.rs:90`. The apparent strict reads in pi-atlas/cache.rs are inside its test module.

## Proof ledger

All log paths are local `/tmp` artifacts. Commands use `-j3`; no live provider calls.

| Revision / scope | Command | Result / log |
|---|---|---|
| Base + architect tests; `fa1c3733d` | `cargo test -j3 -p pi-coding-agent --test architect project_spec_replaces_invalid_utf8` (RED); `... project_spec_` (GREEN) | Two read-boundary failures; five tests pass. `/tmp/utf8-architect-{red,green}.log` |
| Base + changelog test; `406a828b7` | `cargo test -j3 -p pi-coding-agent --test suite_core_1 changelog_replaces_invalid_utf8_without_dropping_sections` (RED); `... differential_changelog::` (GREEN) | Empty changelog failure; four module tests pass. `/tmp/utf8-changelog-{red,green}.log` |
| `406a828b7` + tests; `220fb68a6` | `cargo test -j3 -p pi-coding-agent-extensions --lib bwrap::worker::utf8_tests` | Two RED failures, two GREEN passes. `/tmp/utf8-bwrap-{red,green}.log` |
| `220fb68a6` + tests; `4dbc3ec40` | `cargo test -j3 -p pi-ai --lib utf8_tests` | Three RED failures, three GREEN passes (plus one isolated SSO child). `/tmp/utf8-ai-{red,green}.log` |
| `4dbc3ec40` + bridge tests; `98810464a` | `cargo test -j3 -p pi-coding-agent-extensions --lib utf8_tests` | Eight bridge RED failures, ten GREEN passes including two bwrap tests; exact decoded-copy RED also `/tmp/utf8-bridge-copy-red.log`. `/tmp/utf8-bridge-{red,green}.log` |
| `98810464a` + preview test; `6dc35c077` | `cargo test -j3 -p pi-coding-agent-interactive --lib agent_log_preview_replaces_invalid_utf8` | Displayed read-error RED, exact replacement-text GREEN. `/tmp/utf8-preview-{red,green}.log` |
| `6dc35c077` + artifact test; `5073e4860` | `cargo test -j3 -p pi-coding-agent-extensions --lib complete_artifact_records_replace_invalid_utf8` | Strict-read RED, two-record GREEN. `/tmp/utf8-pyrun-{red,green}.log` |
| Every implementation commit | `cargo fmt --check` | PASS before each commit |
| `5073e4860`, all five touched crates/all targets | `cargo clippy -j3 -p pi-ai -p pi-coding-agent-base -p pi-coding-agent-extensions -p pi-coding-agent-interactive -p pi-coding-agent --all-targets -- -D warnings` | PASS, `/tmp/utf8-clippy.log` |
| Final Rust scope | Manual changed-line Rust readability audit | No new production nesting, suppression, hidden I/O or duplicated branching; byte reads retain existing effect-revealing function names. Existing unrelated complexity unchanged. |
| Final audit docs + unchanged Rust | `python3 scripts/source-inventory.py` | PASS, 686 rows / 186679 lines, `/tmp/utf8-source-inventory.log` |

Later unrelated module fixes do not invalidate earlier bounded tests: their decoder/asserted paths did not change. Final clippy covers the complete Rust tip. The audit-document commit does not invalidate these proofs.
