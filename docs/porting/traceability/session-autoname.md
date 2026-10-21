# session-autoname traceability

Source: complete `packages/coding-agent/extensions/session-autoname/src/index.ts`
at frozen `4f9e6f8a4` ([source lock](../source-lock.json)). Rust:
`crates/pi-coding-agent/src/bundled_extensions/session_autoname.rs`.

The former c248 first-exchange provider implementation is replaced, not retained
as a fallback. No generation prompt, auth lookup, retry, timeout, kickoff or event
hook remains. Factory registration identity is unchanged.

| Source symbol / branch | Rust | Dedicated behavioral proof |
|---|---|---|
| default factory, registered schema, label, instructions, approval false | `session_autoname_extension` | `set_session_name_tool::frozen_registered_tool_matches_results_refusals_and_closure_lifetime` compares registered metadata and executes real tool |
| `normalizeName`: all lines, JS whitespace, no markdown/prefix stripping | `normalize_name` | same differential scenarios |
| `describeRefusal`: blank, agent ID, manager child, ephemeral, explicit clear, preexisting name | `describe_refusal` | same differential scenarios, exact error content/details/isError |
| closure `nameSetByTool`, own repeated/changed name, independent writer, clear, setter failure | factory-local state | same differential scenarios |
| factory lifetime across `session_start`; reset on factory reload | no lifecycle hooks | same differential scenarios; frozen source has no session-start reset |
| `errorResult` and successful result | `name_result` | exact result differential |
| 80 UTF16 units and word boundary | `normalize_name` returns lossless `JsString` | boundary and persistence proof pending |
| `pi.setSessionName` metadata authority and event publication | existing `ExtensionApi::set_session_name` | real-manager cross-reader proof pending |

## Current proof ledger

Initial RED (base `facbfaea`): dedicated differential reaches frozen TS oracle,
then Rust fails because `set_session_name` is absent. Logs and exact revisions:
`/tmp/pi-rust-repin-session-name-tool/ledger.md`.
GREEN, persistence, mutation evidence are not yet claimed. Central coverage row
must not advance until whole-module proof and pending approval criteria are met.

## Pending representation boundary (not an approved deviation)

Valid input of 79 ASCII `x` followed by `🙂` crosses the 80-unit cap inside a
surrogate pair. Normalization retains UTF16 units using `JsString`, without
lossy conversion. Existing extension setter accepts `&str`; tool content/details
use UTF8/`serde_json::Value`. An unpaired surrogate cannot cross either field.
Current code explicitly returns a boundary error rather than silently substituting
U+FFFD. Exact frozen result/storage fixture is pending. This branch is **not
whole-module parity** until that boundary is resolved or explicitly approved.
The old intermediate-string waiver remains pending revalidation; it is not
renewed here.
