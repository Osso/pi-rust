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

05–13 pending. No API gap identified in 02–04. Diagnostics/error and empty-model
branches are ported; current fixtures prove the successful populated paths, not
exhaustive SDK branch parity.
