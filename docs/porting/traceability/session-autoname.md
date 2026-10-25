# session-autoname traceability

Source: `packages/coding-agent/extensions/session-autoname/src/index.ts` at common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/session_autoname.rs`.

| Frozen symbol / branch | Rust | Proof |
|---|---|---|
| default factory, tool metadata/schema/guidelines/approval | `session_autoname_extension` | `suite_extensions::session_autoname::session_name_tool_matches_frozen_registration_refusals_and_updates` compares full registration with real frozen module |
| normalize whitespace, no formatting stripping, 80-unit word/hard limit | `normalize_name` | same differential: multiline/tab, formatting, empty, long words, boundary words, split surrogate, raw UTF-16 |
| empty-name refusal before other checks | `describe_refusal` | whitespace-only case |
| truthy runtime child or header subagent | `describe_refusal` | child, empty child id, subagent cases |
| persisted-only, explicit clear, existing user name | `describe_refusal` | ephemeral, null initial name, existing initial name |
| same-extension rename allowed, external rename/clear denied | `execute_name_tool`, retained tool name | successive names; external name/clear between tool calls |
| write failures reject without updating retained tool name | `execute_name_tool` | failed write followed by successful retry |
| exact success/error content, details and isError presence | `name_result` | every differential result |

The oracle `oracle/session-name-tool-oracle.ts` calls the unmodified frozen extension through its registered tool and records the write boundary and returned values. Rust uses the actual extension runner and a private real SessionManager/control DB. The old provider-issued title-generation implementation, hooks, helpers, faux-provider tests and old-pin oracle calls are removed; they are absent from the common pin. The test is now a module of `suite_extensions` because it does not mutate process-global state.

## Proof ledger

- RED: `/tmp/bundled-name-red.log`: frozen tool registration is missing in the previous Rust module.
- No provider request, timer, or agent-end event is needed or registered.
- Gap: pathological input types and runtime/OS faults are outside the user-observable parity bar; valid UTF-16 tool inputs/results remain lossless.

## Approved intermediate-string gap (2026-10-20)

Historical generated-title proof at the old c248 pin is superseded: that title generator does not exist at the common pin. Tool input, truncation and result strings now retain exact UTF-16, including split surrogate units. The existing SQLite binding name-storage approval remains owned by the [session manager](session-manager.md) and [control DB](session-control-db.md) modules, not by an alternate naming implementation.

- GREEN at `97bb81df3`: 16 exact frozen cases pass in `suite_extensions` (`/tmp/bundled-name-green.log`). After merging parent `77c92996b`, extension-local clippy reaches the crate but finds 120 errors; the one new redundant error conversion was removed. Remaining errors are existing other extension modules (`/tmp/bundled-ext-clippy-after-parent.log`). Rust/TS changed lines reviewed with the readability checklists.
