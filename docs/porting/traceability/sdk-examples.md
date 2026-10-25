# SDK examples 02–13 (Phase E3)

Historical source: `532f7d12`, `packages/coding-agent/examples/sdk/`; old rows are not new-pin acceptance. Current nine-target GREEN17 scope, integration checks and remaining SDK13 inventory: [Q15 checkpoint](../q15-sdk-provider-proof.md).

## 4f9e6f8a4 SDK03 custom prompt

The current failure was a stale native test expectation, not a native prompt bug: frozen03:21–24 supplies a loader custom prompt and clears append files; AgentSession:2586–2597 forwards that loader text to buildSystemPrompt; frozen system-prompt.ts:102–103 appends local date and cwd without a final newline. Native already matches that path. The test now captures the actual example callbacks from the whole-program oracle, invokes the actual frozen prompt builder, and compares the entire provider prompt against the invocation's before/after clock bounds (including a possible midnight boundary). No prompt/date/newline stripping or reconstruction substitutes for frozen output. The append assertion also uses the actual example callback value. Program stdout/disposal remains an explicit SDK-import-boundary proof, not real SDK-state proof. Current common writer-error unit passes; the earlier original-error equality failure is not present in this checkout. At3e0ca6ec, custom-prompt2/2 passes. Subsequent owned SDK-state oracle repins are recorded below; other oracle families remain in the checkpoint inventory.

## Current SDK09 / shared oracle migration

[Contract](../../specs/sdk-examples.md). Frozen09:11–51 creates four sessions and prints the current auth-storage/registry labels; its final inMemory registry reuses default auth storage while custom storage stays independent. Native09 now carries four option/session slots, explicitly binds each registry's storage and adds the missing built-in-only session. Actual RED `q15-sdk-registry-program-red.log` compares native3/old labels against frozen4/current labels. The state oracle now loads real ModelRegistry/AuthStorage, and program observers use real in-memory instances with controlled model inputs rather than a fabricated retired ModelRuntime class. Compat model lookup is actual frozen code. Console formatting now uses actual Node format; SDK02's actual RED shows native JSON double-quote/no-space output versus frozen Node single-quote/spaced output. The [shared inspection authority](../../specs/console-inspect.md) now serves SDK02/04 as well as error arguments; no second formatter is added. At3129cc56, shared inspection14 cases, lossless IO5 and node_fs_errors8 pass; SDK02 four tests pass with literal Node console output. The real SDK04 warning-object fixture passes in the nine-target checkpoint. Its historical multi-skill discovery-order RED used a native enumeration waiver, now superseded by [exact raw-byte directory ordering](package-manager.md#exact-discovery-order-2026-10-25). That older fixture's complete record equality ignores only order; inline-last/filter behavior and raw console equality with identical explicit inputs remain checked. Built-in fixture models are looked up directly, not re-registered without required provider auth. At a9b7b3bb, SDK09 passes with full stdout and shared-key visibility through sessions0/2/3 while custom storage stays isolated. Shared helper migration is not acceptance of remaining SDK targets.

SDK12 expectation correction: frozen12-full-control.ts:27 constructs ModelRegistry.inMemory(authStorage), so its models.json sentinel must not appear even though auth.json credentials remain loaded. Native configure_options already follows that path; only the stale state-oracle request/assertion changes. Its custom prompt also follows the SDK03 date/cwd builder path; a shared actual-frozen-builder assertion replaces the old missing-date/trailing-newline expectation. At81dc7452, SDK12 three tests and SDK03 two tests pass. No SDK12 production change was needed.

Resource-state observer for04/07/08 now passes real AuthStorage and ModelRegistry into the actual frozen SDK, replacing the removed model-runtime import and old constructor fields. At3a5f2e8c, context-files and prompt-template targets pass against actual SDK state; this setup change is not a new console-formatting approval.
Examples compile their shipped files directly in integration tests. Private cwd,
agent directory, in-memory credentials/settings, native extension fixtures and Faux streams
replace live discovery/provider state in fixtures, following 01. Entrypoints remain
opt-in SDK clients, not CLI wrappers. No new dependencies.

`oracle/sdk-examples-oracle.ts` transpiles the complete unchanged pinned program;
SDK imports, process cwd/env/stdout, console log/warn formatting, and the compat
model lookup are replaced. Console argument values, prompt ordering, chosen
models/tools, error names/messages and disposal order are returned. The historical observer printed arrays/objects as JSON; those logs do not prove console formatting. Current observers use actual Node formatting.
Native SDK-added cwd text in system prompts remains intact.

| Source lines / behavior | Native implementation | Behavioral proof |
| --- | --- | --- |
| 02:9–49 runtime/model lookup; available ordering; medium thinking; streamed greeting; finally | `examples/sdk/02_custom_model.rs::run`, `common::stream` | `example_sdk_custom_model`: real authenticated Faux runtime, first available model, exact prompt, complete stdout differential |
| 03:14–75 replace prompt/clear append; append to default; sequential prompts/output; native cleanup/finally | `03_custom_prompt.rs::run`, `resources::options`, `common::stream` | `example_sdk_custom_prompt`: full stdout differential; actual provider contexts assert pirate prompt + cwd, added instructions, exact prompts |
| 04:18–55 filter browser/search; synthetic source; diagnostics; session | `04_skills.rs::run` | `example_sdk_skills`: complete discovered records (historical order-insensitive fixture; current discovery order follows the package-manager trace), filtered results + inline-last skill; matched-input raw stdout differential; no provider calls |

Shared output helper propagates prompt and writer errors and disposes on all
streaming exits. `error::ExampleError` retains Error messages without Debug quotes;
JavaScript source frames are intentionally omitted. Typed native SDK input
boundaries exclude invalid JS values only.

## Proof ledger

Historical commands: `/tmp/pi-rust-sdk-examples/ledger.md` with `NN-*.log`. Current revision/scopes, checks and limits: [Q15 checkpoint](../q15-sdk-provider-proof.md) and `/tmp/pi-rust-repin-delta-ai/q15-proof-ledger.md`. No live providers.

## Remaining scope

02–13 have native examples. Example13 queue callback error handling is revalidated below; runtime extra-abort queue ordering remains pending lane228. Successful transitions remain covered. Optional model lookup/empty availability,
skill-warning, model-fallback-note, empty session listing and absent initial file
branches are implemented but lack independent native fixtures. Those are explicit
proof gaps, not missing APIs or intentionally changed behavior. Common streamed
writer-failure/finally and settings parse-error/stderr paths have differential
fixtures; tests do not claim exhaustive SDK branch parity.

## 05–08 mapping and discrepancies

| Source lines / behavior | Rust example | Test target |
| --- | --- | --- |
| 05:15–48 four ordered tool sets; custom cwd; in-memory sessions/disposal | `05_tools.rs::run` | `example_sdk_tools`: stdout differential; real active tool names including SDK-required end_turn; custom cwd snapshots |
| 06:26–56 configured paths; inline agent-start; streamed prompt; finally | `06_extensions.rs::run` | `example_sdk_extensions`: actual native extension fires before provider stream; full stdout differential |
| 07:15–47 context merge; JS UTF-16 length; count/output; disposal | `07_context_files.rs::run` | `example_sdk_context_files`: real private AGENTS.md with emoji, exact context values and stdout differential |
| 08:17–51 synthetic deploy source/content; merge; output/disposal | `08_prompt_templates.rs::run` | `example_sdk_prompt_templates`: real private template, ordered merge, deploy body and source metadata; stdout differential |

Source bugs classified/preserved: 07 and 08 add one to a discovered list that
already contains the inline resource. Printed counts intentionally exceed actual
counts by one; tests assert this discrepancy, not silently correct it. 07's
comment says disable context but its executable override appends; code wins.

06 uses the approved native ExtensionRegistry instead of executing TS files.
The two configured `.ts` strings remain registry IDs; embedders register native
factories for those IDs. Missing paths remain loader diagnostics as in the source.
Comment-only extension sketches are not executed or registered. Both configured IDs are registered as hidden native factories at existing private
fixture paths; removing the configured paths fails the loaded-ID assertion. The
inline hook is real. 04/07/08/09 return already-disposed session handles for fixture
observation without extra stdout. 05/07/08 also return state snapshots. 04 follows
its source's non-finally output/disposal ordering.

## 09–11 mapping

| Source lines / behavior | Rust example | Test target |
| --- | --- | --- |
| 09:9–34 default/custom runtime, memory sessions, temporary key and disposal | `09_api_keys_and_oauth.rs::run` | `example_sdk_api_keys_and_oauth`: stdout differential, actual default-runtime override visibility, no auth file persisted |
| 10:9–53 disk settings, overrides, low/flush/errors, isolated memory settings | `10_settings.rs::run` | `example_sdk_settings`: stdout differential, actual pre-setter effective overrides, durable low level with overrides absent from disk |
| 11:10–44 memory/new/continue/list/first-three/open | `11_sessions.rs::run` | `example_sdk_sessions`: real private session files, four results prove three-item limit, lazy new-file behavior, full stdout differential |

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
| 12:17–76 custom runtime/key, explicit model, no discovery, off thinking, read/bash, prompt/finally | `12_full_control.rs::run` | `example_sdk_full_control`: full stdout differential; actual provider sees exact custom prompt + cwd, selected tools and prompt |
| 13:20–67 service/runtime factory, initial/new/switch, bind/unsubscribe/queue updates/dispose | `13_session_runtime.rs::{create_runtime,bind_session,run}` | `example_sdk_session_runtime`: full three-stage stdout differential; real service/runtime replacement, distinct new file and original switch target; second test proves non-empty extension binding, queue counts, stale listener removal while the old session remains alive, and explicit unsubscribe |

13 observer IDs are paired with the session they belong to, so rebinding removes
the previous session's subscription, not an equal numeric ID on its replacement.
Native factories use real create_agent_session_services/from_services APIs and
forward session-start reasons and previous file paths. Provider fixtures remain
private. Runtime teardown aborts the outgoing session before invalidating it;
therefore `Queued: 0` precedes each replacement label. Oracle seam behavior is
anchored in pinned `agent-session-runtime.ts:179–186` and
`agent-session.ts:3749–3756`. Final explicit unsubscribe precedes disposal, as in
source on successful output paths. Queue-output failure is an explicit parity gap,
not covered by stdout equality.

## Error-stream and reasoning regressions

`example_sdk_custom_model` also compares a streamed writer failure against the
pinned program: exact Error name/message, unchanged prior stdout, source finally
order and real native session-resource cleanup. Its reasoning-capable Faux model
asserts medium reaches the provider. `example_sdk_full_control` asserts off reaches
the provider without reasoning. No production provider is contacted.

`example_sdk_settings` separately proves console.warn remains stderr, not stdout.
It loads the actual pinned TS SettingsManager on the same private corrupt settings
file and obtains exact error messages. Both TS and Rust leave the corrupt bytes
unchanged; the complete pinned example then receives those real errors, and both
stdout/stderr streams compare exactly. `sdk-settings-state-oracle.ts` is this real
manager seam. Source's successful sessions and warning output order remain intact.

## Accepted review fixes and proof levels

All twelve source ranges were rechecked against the pinned files, including the
03 cwd/agentDir declarations, 04 skill declaration, and 10 cwd declaration.

| Finding | Native observation / proof | Remaining gap |
| --- | --- | --- |
| First streamed writer error | 02 emits two concrete deltas; first error survives and only one delta write occurs. Shared completion arbiter separately receives a later prompt `Err` and preserves the writer error. Both original failure modes have behavioral RED logs. 02/03/06/12 targets pass. | Error precedence uses a pure unit test; the multi-delta/finally test uses the real native session. A provider-stream failure is not a prompt `Err` and is not claimed as such. |
| 03 native disposal | Native session-resource cleanup records two distinct session IDs in execution order; deleting native disposal fails the assertion. | The stdout oracle's disposed IDs no longer stand in for native disposal. |
| 04/07/08 loader attachment | Inspect the actual disposed session's resource loader, system prompt, and prompt templates. Deleting each assignment fails. `sdk-resources-state-oracle.ts` executes the unchanged pinned programs with real pinned loaders/sessions and compares complete skills/context/templates. | Fixture roots, settings/runtime/model selection and console handling remain injected; no live prompt/provider runs in these three examples. |
| 06 configured extensions | Hidden factories at both configured private paths record exact loaded IDs and order. Deleting configured paths fails. | Native factories intentionally replace TS extension loading. |
| 09 custom runtime | Custom session has no injected runtime. Actual session runtime reads private `auth.json` and `models.json`; wrong filenames independently fail. `sdk-runtime-state-oracle.ts` reads the same files through real pinned ModelRuntime and compares model/auth values. | Default runtime is still the isolated fixture runtime. |
| 12 defaults/key | No injected model/runtime. Default Sonnet matches the real pinned compat lookup. Runtime reads private auth/models files. Child processes cover nonempty, empty, and absent `MY_ANTHROPIC_KEY`, exact resolved key, and unchanged auth bytes. Wrong model/path/env lookup each fail. | Default fixture root substitutes for `/tmp/my-agent`; no production provider is contacted. |
| 13 binding/rebinding | Non-empty native extension records session_start only after binding. Independent replacement runtime leaves the old session undisposed; a separate observer proves it still emits when testing stale-listener removal. Omitting binding and omitting unsubscribe independently fail. | Queue-output failure ordering remains blocked below. |

09 and 12 use an explicitly supplied `agent_dir` as the private auth/models root
when constructing their fixture runtime, retaining the exact source filenames.
Without that fixture input, executable defaults remain `/tmp/my-app` (09) and
`/tmp/my-agent` (12). No test reads those shared locations. Runtime construction
and option assembly are exposed for observation; executable stdout is unchanged.

### SDK13 queue-output revalidation (2026-10-23)

Historical failure-before-disposal claim is **stale**, not a current-pin gap. Frozen `agent-session.ts:1476–1484` catches/logs subscriber failures in registration order; `agent-session-runtime.ts:220–230` disposes without calling abort. No fallible public subscriber API is needed for example13.

`sdk-runtime-order-oracle.ts` constructs real pinned AgentSession/Runtime, binds a non-empty extension, explicitly calls `clearQueue()` to invoke a throwing listener, then replaces the session successfully:

```json
{"error":null,"disposed":true,"replacements":1,"sameSession":false,"bindStarts":1}
```

Native example13 no longer defers writer errors and rejects after replacement. Its queue callback reports `Session event listener error (queue_update): Error: queue output failed` on stderr and continues, matching the frozen catch (V8 stack frames omitted under the approved gap). `example_sdk_session_runtime::queue_output_failure_does_not_interrupt_replacement` compares the real source boundary, checks successful native execution, and invokes an isolated native child to compare exact report text/sink against actual source stderr. RED `q15b-sdk13-current-queue-red.log` → GREEN `q15b-sdk13-explicit-queue-green.log`.

Lane228's runtime correction (parent72ecf55b) removes the extra abort. Post-merge RPC full-frame differential passes (`q15b-sdk13-rpc-post-runtime.log`); SDK13 program observer initially exposed its own stale simulated abort, now removed against frozen:220–230. `q15b-sdk13-current-program-green.log` records exact stdout GREEN. Explicit real queue-error/report proof remains separate; no agent_session_runtime.rs edits in this lane.


## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).
