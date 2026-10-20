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

09–13 pending. No API gap identified in 02–04. Diagnostics/error and empty-model
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
