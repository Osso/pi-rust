# Extension examples G — modes

Pinned source: `532f7d12`, opt-in native embedding; no default registration.
Tests include actual example source via `#[path]`, explicit registry factories,
real `ExtensionRunner` and isolated state inherited from Cargo's runner.

| Source / entrypoint | Native source | Behavioral proof |
| --- | --- | --- |
| `plan-mode/utils.ts`: `isSafeCommand`, destructive/read-only patterns | `plan_mode/utils.rs` | `example_plan_mode_utils`: 500+ corpus cases against unchanged pinned pure helpers |
| `cleanStepText`, `extractTodoItems`, `extractDoneSteps`, `markCompletedSteps` | `plan_mode/utils.rs` | Same corpus: markup, headers, numbered plans, whitespace, length boundaries, duplicate/missing DONE steps, binary64 rounding |
| `minimal-mode.ts`: all seven tool registrations | `minimal_mode.rs` | `example_minimal_mode`: complete ordered tool name/label/description/schema comparison |
| Each minimal tool's `renderCall`, `renderResult` | Same | 168 rendered call/result pairs, empty/populated args, expanded/collapsed, absent/empty/multiline/error text; complete rendered lines including ANSI/padding |
| Each minimal tool's `execute` | Same | Real loader/runner and built-ins on private files: read/write/edit/find/grep/ls/bash; complete results and resulting file contents |

## Discrepancies and exclusions

- Native extensions replace dynamic TS loading (approved project deviation).
- Typed Rust tool inputs exclude invalid JS types, per project rules.
- No live providers/config/control DB: private temp files, in-memory credentials,
  model runtime with networking disabled, in-memory session manager.
- Plan helper UTF-16 truncation currently uses the existing compatibility helper;
  split-surrogate boundary remains unproven and is not claimed as parity evidence.
- Minimal bash's expanded empty text still contains color escapes and renders an
  empty colored line. Preserve this source behavior, rather than simplifying it.

## Proof ledger

Full commands, source scope, RED/GREEN/mutation evidence: `/tmp/pi-rust-ext-examples-g/ledger.md`.
Mutation checks change completion count and minimal limit text, each fails its
respective differential, then original source is restored.
