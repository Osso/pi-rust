# session-autoname traceability

Source: complete `packages/coding-agent/extensions/session-autoname/src/index.ts`
at frozen `4f9e6f8a4` ([source lock](../source-lock.json)). Rust:
`crates/pi-coding-agent/src/bundled_extensions/session_autoname.rs`.

The former c248 first-exchange provider implementation is replaced, not retained
as a fallback. Its dedicated `tests/session_autoname.rs` and
`oracle/session-autoname-oracle.ts` are removed: their old title-helper/request
assertions do not describe the frozen module. A repository reference scan found
no shared matrix calls to those helpers or oracle; only bundled factory wiring
uses the retained `session_autoname_extension` entry point. New-tool differential,
real API/events and cross-reader tests replace that baseline surface. No generation prompt, auth lookup, retry, timeout, kickoff or event
hook remains. Factory registration identity is unchanged.

| Source symbol / branch | Rust | Dedicated behavioral proof |
|---|---|---|
| default factory, registered schema, label, instructions, approval false | `session_autoname_extension` | `set_session_name_tool::frozen_registered_tool_matches_results_refusals_and_closure_lifetime` compares registered metadata and executes real tool |
| `normalizeName`: all lines, JS whitespace, no markdown/prefix stripping | `normalize_name` | same differential scenarios |
| `describeRefusal`: blank, agent ID, manager child, ephemeral, explicit clear, preexisting name | `describe_refusal` | same differential scenarios, exact error content/details/isError |
| closure `nameSetByTool`, own repeated/changed name, independent writer, clear, setter failure | factory-local state | same differential scenarios |
| factory lifetime across `session_start`; reset on factory reload | no lifecycle hooks | same differential scenarios; frozen source has no session-start reset |
| `errorResult` and successful result | `name_result` | exact result differential |
| 80 UTF16 units and word boundary | `normalize_name` returns lossless `JsString` | `utf16_whitespace_and_word_boundaries_match_the_executed_ts_tool`: 9 exact ordinary boundaries; split-surrogate output remains blocked |
| `pi.setSessionName` metadata authority and event publication | existing `ExtensionApi::set_session_name` | `public_ts_manager_and_rust_tool_share_metadata_in_both_directions`, `real_extension_api_persists_publishes_events_and_never_requests_a_title` |

## Current proof ledger

Initial RED (base `facbfaea`): dedicated differential reaches frozen TS oracle,
then Rust fails because `set_session_name` is absent. Logs and exact revisions:
`/tmp/pi-rust-repin-session-name-tool/ledger.md`.
Ordinary differential GREEN at `8f12adea`: one test, 13 scenarios, no warnings.
At `ad451431`, all five targeted tests pass without warnings: 15 lifecycle/refusal
scenarios, 9 whitespace/UTF16 boundaries, TS-created session → Rust tool → same TS
reader; independent TS name takeover refused; Rust-created session → Rust tool →
TS reader. Real ExtensionApi test publishes both name events and asserts zero
faux-provider calls before naming a completed first exchange and after two tool
calls. The current transcript fixture additionally appends the exact frozen
result through public `SessionManager.appendMessage` and inspects actual JSONL
bytes; its latest execution is recorded in the ledger.

Ten source mutants were killed by behavioral assertions (not compilation): blank,
child, ephemeral, explicit-clear, preexisting-name, own-rename, UTF16 cap,
whitespace, persistence and erroneous session-start reset. An exact copy of the
owned module first passed all five tests; then each independently mutated copy
ran the same target with jobs4 and mandatory sandbox runner. Production/shared
files were not mutated. Exact edits, SHA256 values and outputs are retained in
`/tmp/pi-rust-repin-session-name-tool/mutations.json` and individual logs.

Typed string inputs are the contract; invalid JavaScript runtime argument types
remain the repository's explicit typed-API exclusion. No central coverage row
may advance until whole-module proof and pending approval criteria are met.

## Pending representation boundary (not an approved deviation)

Valid input of 79 ASCII `x` followed by `🙂` crosses the 80-unit cap inside a
surrogate pair. Normalization retains UTF16 units using `JsString`, without
lossy conversion. Existing extension setter accepts `&str`; tool content/details
use UTF8/`serde_json::Value`. An unpaired surrogate cannot cross either field.
Current code explicitly returns a boundary error rather than silently substituting
U+FFFD. `split_surrogate_is_proven_exactly_and_not_silently_converted` executes the
frozen registered tool with a real TS manager: tool text/details and manager name
end with `\\ud83d`, while its SQLite read ends with U+FFFD. The exact JSON fixture
is printed in the targeted test log. Public manager append is also required to
retain `\\ud83d` in the persisted tool-result text/details, not SQLite's U+FFFD;
this is a separate output boundary from the metadata name. This branch is **not
whole-module parity** until that boundary is resolved or explicitly approved.
The old intermediate-string waiver remains pending revalidation; it is not
renewed here.
