# Extension examples G — modes

Pinned source: `532f7d12`, opt-in native embedding; no default registration.
Tests include actual example source via `#[path]`, explicit registry factories,
real `ExtensionRunner` and isolated state inherited from Cargo's runner.

| Source / entrypoint | Native source | Behavioral proof |
| --- | --- | --- |
| `plan-mode/utils.ts`: `isSafeCommand`, destructive/read-only patterns | `plan_mode/utils.rs` | `example_plan_mode_utils`: 500+ corpus cases against unchanged pinned pure helpers |
| `cleanStepText`, `extractTodoItems`, `extractDoneSteps`, `markCompletedSteps` | `plan_mode/utils.rs` | Same corpus: markup, headers, numbered plans, whitespace, length boundaries, duplicate/missing DONE steps, binary64 rounding |
| `minimal-mode.ts`: all seven tool registrations | `minimal_mode.rs` | `example_minimal_mode`: complete ordered tool name/label/description/schema comparison |
| Each minimal tool's `renderCall`, `renderResult` | Same | 392 rendered call/result pairs, empty/populated args, expanded/collapsed, absent/empty/multiline/error text; complete rendered lines including ANSI/padding |
| Each minimal tool's `execute` | Same | Real loader/runner and built-ins on private files: read/write/edit/find/grep/ls/bash; complete results/errors and resulting file contents |

## Discrepancies and exclusions

- Native extensions replace dynamic TS loading (approved project deviation).
- Typed Rust tool inputs exclude invalid JS types, per project rules.
- No live providers/config/control DB: private temp files, in-memory credentials,
  model runtime with networking disabled, in-memory session manager.
- Plan helper text uses `pi_js_compat::js_json::JsString`; split-surrogate
  truncation is lossless. Unicode regex boundaries/ASCII case folding are tested.
- Minimal bash's expanded empty text still contains color escapes and renders an
  empty colored line. Preserve this source behavior, rather than simplifying it.

## Proof ledger

Full commands, source scope, RED/GREEN/mutation evidence: `/tmp/pi-rust-ext-examples-g/ledger.md`.
Mutation checks change completion count and minimal limit text, each fails its
respective differential, then original source is restored.

## Preset

`preset.rs` exports `preset_extension`. `example_preset` compares complete
ordered command/flag/shortcut metadata and action/UI transcripts through the real
TS and Rust loader/runner, plus final tools/thinking/model/session entries.

| Entrypoint / branch | Proof |
| --- | --- |
| `session_start`, global/project config merge | Missing configs; global presets overridden by project; restored preset name; explicit/empty/unknown flag |
| `/preset <name>`, `applyPreset` | Known/unknown presets, model found/missing, model action succeeds/fails, thinking changes, valid/invalid/empty tools |
| `/preset`, selector | Complete themed rendered component; enter/down/escape; active label; None clearing; no-presets warning |
| Ctrl+Shift+U / cycle | Sorted cycle through all presets and None; original tools/thinking restoration |
| `before_agent_start` | Full system prompt and messages output, with/without instructions |
| `turn_start` | Persisted custom entry name and complete final session entries |
| Config load errors | Both malformed global/project JSON: exact console error text via local logger seam |

Selector reconstructs Rc-based TUI components on the UI thread while preserving
selection as shared scalar state, satisfying the existing native Send+Sync UI API.
No dynamic TS loading or default registration. Models are faux registered fixture
models, no network/provider completion calls.

## Plan-mode blocked — not shipped

TS `plan-mode/index.ts:354` aliases `planModeEntry.data.todos`;
`index.ts:382` and `utils.ts:159–165` mutate those same todo objects when restoring
completion. Existing session entries therefore change in memory. Differential log
`14-plan-green.log` finds identical UI/action traces but different complete session
entries: TS's restored saved todo has `completed:true`, native remains `false`.

Missing API: shared/mutable custom-entry data (or custom-entry data update).
Native `src/core/session_manager.rs:3072–3079` returns cloned entries;
`:2964–2976` only appends owned custom data. `ExtensionApi::append_entry` at
`src/core/extensions/types.rs:3197–3200` also accepts owned `Option<Value>`.
No public operation mutates existing custom-entry data. Core API changes are out
of this batch's permitted scope; the incomplete plan-mode implementation was
removed. `example_plan_mode` is a behavioral discrepancy reproduction, not parity.
Do not mark `plan-mode/index.ts` done without classifying this source aliasing
behavior or exposing an approved native API.

## Final targeted results

| Target | RED | GREEN / latest scope | Mutation |
| --- | --- | --- | --- |
| `example_plan_mode_utils` | Missing module; later lossless Unicode regression | 1 passed; >500 input cases, all five helpers, ba98058 | Completion count changed to zero: test fails; restored |
| `example_minimal_mode` | Missing module | 1 passed; 392 call/result render pairs, seven successes plus seven errors, 9ed3074 | Limit text changed: test fails; restored |
| `example_preset` | Missing module | 1 passed; config/CLI/shortcut/custom UI/redraw/model/thinking/tools/prompt/persistence/errors, 9ed3074 | Activation text changed: test fails; restored |
| `example_plan_mode` | Attempted port: 2 failures; one exposed API gap | 1 passed discrepancy-only reproduction; NO native plan-mode port | Not applicable: stopped, incomplete source removed |

No new dependencies, live state, providers, default registration, shared core API
changes, agents, model CLIs, merge, or push. Only targeted Cargo tests and fmt run.
No clippy/check/workspace suites. Plan-mode remains blocked, not done.

## Accepted review regression (2026-10-20)

F1: `example_minimal_mode` uses float-backed offset 2.0, limit 3.0 and timeout 2.0 in full seven-tool call/result comparisons. Number arithmetic/formatting uses JS f64 semantics, including read range :2-4.

## Q18 plan-mode utility native text (2026-10-22)

`examples/extensions/plan_mode/utils.rs` now uses the shared UTF-16 `JsRegex` engine instead of scalar `fancy_regex` plus ASCII/whitespace expansion shims. Public helper inputs become native before matching; cleaning, first-unit uppercase and 47-unit truncation retain raw units. Existing source aliasing gap above remains unchanged.

| Frozen utility branches | Native helper | Proof target |
|---|---|---|
| destructive/safe patterns, JavaScript case/word/whitespace semantics | `is_safe_command`, native constant regexes | `example_plan_mode_utils` pinned differential |
| formatting removal, prefix removal, uppercase first unit, truncation | `clean_step_text` | `example_plan_mode_utils` pinned differential |
| plan header/numbered steps, short/prefix exclusions | `extract_todo_items` | `example_plan_mode_utils` pinned differential |
| numeric DONE tags, finite filtering, completion updates | `extract_done_steps`, `mark_completed_steps` | `example_plan_mode_utils` pinned differential |
