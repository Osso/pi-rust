# SDK examples 02–13 (Phase E3)

Pinned source: `532f7d12`, `packages/coding-agent/examples/sdk/`.
Examples compile their shipped files directly in integration tests. Private cwd,
agent directory, in-memory credentials/settings, empty extensions and Faux streams
replace live discovery/provider state in fixtures, following 01. Entrypoints remain
opt-in SDK clients, not CLI wrappers. No new dependencies.

`oracle/sdk-examples-oracle.ts` transpiles the complete unchanged pinned program;
only SDK imports are replaced. Console argument values, prompt ordering, chosen
models/tools, error names/messages and disposal order are returned. Arrays and
objects print as JSON rather than Node inspect syntax; no values are omitted.
Native SDK-added cwd text in system prompts remains intact.

| Source lines / behavior | Native implementation | Behavioral proof |
| --- | --- | --- |
| 02:9–49 runtime/model lookup; available ordering; medium thinking; streamed greeting; finally | `examples/sdk/02_custom_model.rs::run`, `common::stream` | `example_sdk_custom_model`: real authenticated Faux runtime, first available model, exact prompt, complete stdout differential |
| 03:16–75 replace prompt/clear append; append to default; sequential prompts/output; finally | `03_custom_prompt.rs::run`, `resources::options`, `common::stream` | `example_sdk_custom_prompt`: full stdout differential; actual provider contexts assert pirate prompt + cwd, added instructions, exact prompts |
| 04:19–55 filter browser/search; synthetic source; diagnostics; session | `04_skills.rs::run` | `example_sdk_skills`: private disk discovery of three skills; filtered ordered names + inline skill; stdout differential; no provider calls |

Shared output helper propagates prompt and writer errors and disposes on all
streaming exits. `error::ExampleError` retains Error messages without Debug quotes;
JavaScript source frames are intentionally omitted. Typed native SDK input
boundaries exclude invalid JS values only.

## Proof ledger

Development commands, revisions/scopes and results:
`/tmp/pi-rust-sdk-examples/ledger.md` with `NN-*.log`. Only per-example test targets
and formatting run; no broad checks, clippy or live providers.

## Remaining scope

02–13 implemented. No API gap identified in 02–04. Diagnostics/error and empty-model
branches are ported; current fixtures prove the successful populated paths, not
exhaustive SDK branch parity.

## 05–08 mapping and discrepancies

| Source lines / behavior | Rust example | Test target |
| --- | --- | --- |
| 05:20–56 four ordered tool sets; custom cwd; in-memory sessions/disposal | `05_tools.rs::run` | `example_sdk_tools`: stdout differential; real active tool names including SDK-required end_turn; custom cwd snapshots |
| 06:31–64 configured paths; inline agent-start; streamed prompt; finally | `06_extensions.rs::run` | `example_sdk_extensions`: actual native extension fires before provider stream; full stdout differential |
| 07:17–53 context merge; JS UTF-16 length; count/output; disposal | `07_context_files.rs::run` | `example_sdk_context_files`: real private AGENTS.md with emoji, exact context values and stdout differential |
| 08:21–63 synthetic deploy source/content; merge; output/disposal | `08_prompt_templates.rs::run` | `example_sdk_prompt_templates`: real private template, ordered merge, deploy body and source metadata; stdout differential |

Source bugs classified/preserved: 07 and 08 add one to a discovered list that
already contains the inline resource. Printed counts intentionally exceed actual
counts by one; tests assert this discrepancy, not silently correct it. 07's
comment says disable context but its executable override appends; code wins.

06 uses the approved native ExtensionRegistry instead of executing TS files.
The two configured `.ts` strings remain registry IDs; embedders register native
factories for those IDs. Missing paths remain loader diagnostics as in the source.
Comment-only extension sketches are not executed or registered. The native inline
hook is real. 05/07/08 return state snapshots to fixture callers without adding
stdout lines. 04 now follows its source's non-finally output/disposal ordering.

## 09–11 mapping

| Source lines / behavior | Rust example | Test target |
| --- | --- | --- |
| 09:9–37 default/custom runtime, memory sessions, temporary key and disposal | `09_api_keys_and_oauth.rs::run` | `example_sdk_api_keys_and_oauth`: stdout differential, actual default-runtime override visibility, no auth file persisted |
| 10:11–58 disk settings, overrides, low/flush/errors, isolated memory settings | `10_settings.rs::run` | `example_sdk_settings`: stdout differential, actual pre-setter effective overrides, durable low level with overrides absent from disk |
| 11:12–58 memory/new/continue/list/first-three/open | `11_sessions.rs::run` | `example_sdk_sessions`: real private session files, four results prove three-item limit, lazy new-file behavior, full stdout differential |

11 additionally runs real pinned TS SessionManager.continueRecent and list,
separately, on the exact same files Rust reads. Fixture byte identity is asserted
before/after both implementations. Both selection and list order match; continuation
and listing need not agree with each other (different file/metadata ordering).
`sdk-sessions-state-oracle.ts` imports only the pinned manager, no SDK mocks.
The runner-private default DB is used consistently; fixture cwd/session dirs remain
explicit. 09 runtime creation and 10 settings I/O occur only at opt-in entrypoints
or private fixture paths. Runtime API keys are ephemeral, not written to auth.json.
10 returns its effective override snapshot before the source's later setter; the
setter may remerge disk state, which the example does not change.

## 12–13 mapping

| Source lines / behavior | Rust example | Test target |
| --- | --- | --- |
| 12:23–76 custom runtime/key, explicit model, no discovery, off thinking, read/bash, prompt/finally | `12_full_control.rs::run` | `example_sdk_full_control`: full stdout differential; actual provider sees exact custom prompt + cwd, selected tools and prompt |
| 13:23–67 service/runtime factory, initial/new/switch, bind/unsubscribe/queue updates/dispose | `13_session_runtime.rs::{create_runtime,bind_session,run}` | `example_sdk_session_runtime`: full three-stage stdout differential; real service/runtime replacement, distinct new file and original switch target; second test proves queue counts, stale listener removal and explicit unsubscribe |

13 observer IDs are paired with the session they belong to, so rebinding removes
the previous session's subscription, not an equal numeric ID on its replacement.
Native factories use real create_agent_session_services/from_services APIs and
forward session-start reasons and previous file paths. Provider fixtures remain
private. Runtime teardown aborts the outgoing session before invalidating it;
therefore `Queued: 0` precedes each replacement label. Oracle seam behavior is
anchored in pinned `agent-session-runtime.ts:179–186` and
`agent-session.ts:3749–3756`. Final explicit unsubscribe precedes disposal, as in
source. No new API gap or semantic deviation identified.
