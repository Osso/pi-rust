# Interactive user-visible parity — active successor

## Authorized scope

User-observable `interactive-mode.ts` startup/render/events and command/session-transition slices, `agent-session-runtime.ts`, `self-restart.ts` and `keybindings.ts`, against common pin `4f9e6f8a4`. Keep existing lossless carriers. Mechanical cross-lane adapters are cleared; semantic deviations/dependencies/TS deltas still need approval. No agents, full-suite runs, push, deploy or live providers. Parent owns integration and independent verification. The current merge train requires a compiling workspace/all-targets tip before each commit and permits periodic parent-tip merges.

## Baseline repair checkpoint (2026-10-25)

- `fbe31b1ec`: fixture streams capture their own provider instead of the global `faux` API. Deterministic RED receives another session's answer after registration/refresh; GREEN preserves each complete turn. This fixes the notification waits behind both baseline hangs and the tree/fork fixture collision without timeouts or serialization.
- `da8ea24dc`: authentication snapshot assertions use existing lossless `JsValue` Number semantics. Full model/count/chat/error objects are unchanged; all nine source scenarios pass. The failures were serde `Number(1.0)` versus `Number(1)` equality, not product values.
- `aa2a3af94`: live expansion now follows frozen :5411–5425: flag → active header → resource/chat children → render, without the old output-padding write. Ctrl+O reveals the background-tool hint and Ctrl+B still detaches. The actual header oracle covers 48 complete original frames across four startup states and four header registrations/restorations.
- `ac3c00885` plus formatting follow-up: strict types for the AST/oracle and source-selected compiler proof. No frozen source changes.

Proof ledger root: `/tmp/repin-interactive-2-proof/`. Rust source scope is `aa2a3af94`; later oracle-only type/format edits do not invalidate Rust compilation/clippy or the other eight tests.

| Command / scope | Result | Artifact / invalidation |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent-interactive --lib --` with the two hangs, two auth tests, background keybinding, tree, fork, fixture ownership and header filters | 9/9 parallel, no ignores, 14.27s | `baseline-parallel-green.log`; current Rust and pre-type-only oracle |
| Same command, only header filter, final oracle | 1/1; all 48 frames | `header-final-runtime.log` |
| `cargo check -j4 -p pi-coding-agent-interactive --lib` | PASS, zero warnings/errors | `check.log`; current Rust |
| `cargo clippy -j4 -p pi-coding-agent-interactive --lib --tests --no-deps --message-format=json` | PASS; 102 existing diagnostics, zero primary spans intersect changed Rust lines | `clippy-baseline.{jsonl,stderr}`; current Rust; not whole-crate warning-clean |
| Frozen `tsgo --project` via `buildSourceTsconfig`, header oracle only, target/lib ES2024 under oracle-gate approval | PASS, strict/noEmit, original source/compiler | `header-final-typecheck.log`; formatting-only follow-up does not change types |
| Frozen Biome, header oracle only | PASS | `header-biome.log` records initial info; final formatter applied and clean check reported at checkpoint |
| `cargo fmt --all --check` | PASS before each commit | Current Rust |
| `python3 scripts/source-inventory.py` | PASS, 686 / 186679; 467 done / 19 gap / 47 partial / 153 todo | Baseline checkpoint; no status promotions |

Changed-line readability review: fixture ownership is explicit; expansion is a short ordered sequence and one bounded native-type dispatcher; oracle frame capture names each surface and transition. No new warning suppression, dependency, alternate provider path or carrier is added.

## Actual startup checkpoint (2026-10-25)

`4d0769bf5` invokes the full init/rebind/resource-discovery path before resource listing and restored messages, registers resource themes after binding and prints the missing scoped-model console line. `96de499d3` installs theme invalidate/border/render and footer branch-change callbacks; `da4b17ada` names the cycle-hint formatter. The real frozen SDK/AgentSession/runtime/InteractiveMode and real native DefaultResourceLoader/runtime/mode match seven complete header/resource/chat/title/stdout/provider-count/discovered-skill snapshots. Private theme and Git HEAD changes additionally prove repaint side effects and full post-theme editor frames. The ignored process helper is always invoked by its unignored parent; no scenario is ignored.

Current Rust proof scope: `da4b17ada`; artifacts remain under `/tmp/repin-interactive-2-proof/`.

| Command / scope | Result | Artifact |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent-interactive --lib --` with baseline regressions, actual startup, header, reload, rebind and loaded-resource filters | 12/12 parallel; 7 startup process scenarios; 9.42s | `startup-integration.log` |
| `cargo check -j4 -p pi-coding-agent-interactive --lib` | PASS, zero warnings/errors | `startup-check.log` |
| Scoped clippy command above, after startup edits | PASS; 102 existing diagnostics, zero changed-line primary spans | `startup-clippy.{jsonl,stderr}` |
| Strict source-selected startup oracle typecheck, ES2024 as approved | PASS | `startup-final-typecheck.log` |
| Frozen Biome startup oracle + Rust formatting | PASS, no suppressions | final command output; `cargo fmt --all --check` before each commit |
| Source inventory | PASS, 686 / 186679; 467 done / 19 gap / 48 partial / 152 todo | interactive-mode now honestly partial |

Readability: source scope printing and cycle-hint formatting are separate short steps; callback subscription and UI-owner delivery use existing carriers; fixtures use private state/processes and actual resource discovery. No new dependency, public carrier, unapproved semantic gap or cross-lane algorithm change.

## Keybindings checkpoint

`8523469b9` (integrated by parent `f78e73229`) closes the 444-line coding-agent keybindings module: full exported catalog/default/description comparison, five migrations, four file loads and eight repeated reload/conflict/match snapshots; read-only file bytes and native UTF-16 are retained. Parent approved removal of the old-only copy action and macOS ordering; the clipboard description now matches exactly. Focused gates: `keybindings-suite_core_{2,3,4}.log` = 1/1 differential + 3/3 units + 1/1 raw file proof; `keybindings-check.log` = zero warnings/errors; `keybindings-clippy.{jsonl,stderr}` = 104 existing diagnostics, zero changed lines; strict TS/Biome pass. Source inventory at that checkpoint: 686 / 186679, 468 done / 19 gap / 48 partial / 151 todo. Parent later fast-forwarded this worktree to `1425a937c`; the newer global inventory below supersedes these historical status counts.

## Typed missing-import checkpoint

`6cbc0bafa` preserves the existing missing-import error's name/message/filePath and original type through existing carriers. Full runtime source/native RED reproduces generic Error + lost property; actual persisted-mode process RED reproduces a factory Error incorrectly treated as recoverable merely because its text starts `File not found:`. Missing import exits0 and factory failure exits1, with full exact real-mode error frames, after the fix. No new error class/carrier/dependency is added. Other import-command confirmation, parsing and missing-cwd branches are not accepted by this bounded proof.

| Command / scope at `6cbc0bafa` | Result | Artifact |
|---|---|---|
| `cargo test -j4 -p pi-coding-agent --test agent_session_runtime -- --nocapture` | 45 passed; one explicitly parent-invoked isolated cwd helper, no omitted scenario | `import-runtime-integration.log` |
| Interactive lib, import process + actual startup filters | 2/2; two import process cases + seven startup scenarios | `import-ui-integration.log` |
| `cargo check -j4 -p pi-coding-agent-interactive --lib` | PASS, zero warnings/errors | `import-check.log` |
| Scoped storage/core/interactive lib+tests clippy, no-deps JSON | PASS; 307 existing diagnostics, zero changed-line primary spans | `import-clippy.{jsonl,stderr}` |
| Strict two-oracle frozen TS/ES2024 + Biome | PASS | `import-typecheck.log` |
| Format + locked inventory | PASS; 686 / 186679; 484 done / 21 gap / 52 partial / 129 todo | Inventory before checkpoint; runtime honestly partial |

Diagnostic incident: an early oracle waited at the real confirmation dialog because the fixture had not driven terminal input. Both owned process groups were killed, and all six recorded PIDs were verified exited. The final fixture drives Enter through the actual terminal callback; no dialog stub, production timeout or assertion weakening was used. An initial in-memory import probe hit the source's `mkdir('')` boundary before the factory, so the like-boundary comparison now uses persisted sessions on both sides.

## Interactive import checkpoint

`14bc35da4` + `3e36e1421` port literal UTF-16 path parsing and the full realistic import dialog/retry workflow. Source/native proof covers 26 parser vectors and 13 import process outcomes, including exact copied JSONL bytes/restored messages, cwd retries, cancellation, missing/factory error classification and raw fatal values. The source's in-memory empty-directory mkdir failure is preserved explicitly. Shared export argument parsing is covered; whole export handling is not accepted here.

Final proof at `3e36e1421`: `import-command-final-ui.log` has eleven passing focused UI tests, including all import and startup scenarios; its header test exposed a process-global keybinding fixture collision (native hints reverted to ctrl+c/ctrl+o instead of the configured ctrl+x/ctrl+y). `336ef21d8` moves that unchanged 48-frame differential into an explicitly invoked child process, without serializing tests or changing production behavior. `import-header-isolated-green.log` proves header + concurrent switch/fork 2/2. No assertion is weakened and no scenario is omitted.

`import-command-final-runtime.log`: 45/45, plus the existing explicitly invoked isolated cwd helper. `import-command-check.log`: PASS without warnings. Scoped core/interactive clippy (`import-command-clippy.jsonl`): 232 existing diagnostics, no changed-line primary span; after the test-only isolation change, interactive clippy (`import-header-clippy.jsonl`): 204 existing diagnostics, no changed-line primary span. Strict frozen two-oracle TS/ES2024 and Biome pass (`import-command-typecheck.log`). Format/inventory pass: 686 / 186679; 485 done / 21 gap / 52 partial / 128 todo. Artifacts remain under `/tmp/repin-interactive-2-proof/`. Runtime and interactive mode remain partial.

## Setup and transfer checkpoints

`cd317f852` completes the approved live SharedSessionManager setup correction. Callback differential3/3; requested workspace all-targets0 errors/0 warnings; strict TS/Biome/format/inventory pass; scoped tools/core clippy259 existing diagnostics, zero changed-line primary spans. No code changed after that workspace proof until the independent transfer slice.

Transfer behavior through `092fb53a3`: [full source/RED/GREEN/gate ledger](traceability/interactive-mode.md#transfer-commands-2026-10-25). Eleven actual process cases cover export/copy/share, including Japanese/lone-unit JSONL bytes, copied clipboard bytes, the real Esc RefCell crash correction and graceful SIGTERM. Shared-loader component differentials also pass. Whole interactive mode remains partial. The resource block owned by276 is untouched.

## Name/inventory checkpoint

`acefd5e8c` mechanically moves both CLI formatter implementations unchanged into the tools layer, with facade re-exports; `f80e52736` ports name/unname/tools/extensions handlers. [Full proof](traceability/interactive-mode.md#name-and-inventory-commands-2026-10-25):16 source/native states at widths25/120, persisted names/events/title effects and repeated inventories; lower formatter3+4 common-pin tests. Required -j3 workspace/all-targets0/0, TS/Biome/fmt/inventory pass, scoped clippy0 owned findings. No changes to cli/args.rs or main.rs for this slice.

## Help/notices checkpoint

`9819acb69` closes the bounded help/notices group:17 actual-source cases, including full-file changelog instead of cached startup excerpt, empty-version/empty-notice truthiness and PI_OFFLINE empty-string behavior. Default/custom hotkeys and extension descriptions match unchanged helpers; only the classified User-Agent runtime token is adapted. [Full ledger](traceability/interactive-mode.md#help-and-startup-notices-2026-10-25). Required pre-commit workspace -j3 all-targets0/0, TS/Biome/fmt pass, clippy0 owned findings. Whole interactive-mode remains partial.

## Interleaved event display checkpoint

`689fc1a50`: the bounded [event-display contract](../specs/interactive-event-display.md) now has twelve actual-source/native event steps and24 complete three-surface frames at widths25/120. RED reproduces premature tool rows/final-order changes, empty retry/compaction truthiness errors and missing provider retry/fallback details. GREEN preserves complete UTF-16 frames and terminal-progress calls. This does not close the event table or message replay.

| Command / Rust scope `689fc1a50` | Result | Artifact |
|---|---|---|
| Focused interactive lib `interleaved_text_tool_and_retry_compaction_frames_match_frozen` | 1/1, twelve steps at both widths | `event-frames-final.log`; earlier `event-frames-red-expanded.log` |
| `cargo check -j3 --workspace --all-targets --message-format=json` | PASS before commit | `event-workspace.log` |
| `cargo fmt --all --check`; source inventory | PASS;686/186679;634 done/25 gap/18 partial/9 todo | Current checkpoint |

Post-commit strict TS and Biome pass on the unmodified common pin with the approved target/lib ES2024 (`event-{typecheck,biome}-final.log`). Scoped interactive lib/tests clippy passes with100 existing diagnostics and zero changed-line primary findings (`event-clippy*`). Manual changed-line Rust/TS readability audit is complete; the optional metric binary is not installed. The new formatter is a short pure function; sequential event capture remains ordered. Source changes invalidate this proof; documentation-only changes do not. No carrier, dependency, TS delta or adjacent lane algorithm changes.

## Recorded replay follow-up

`d29e15875` extends the same actual-source/capture regression with three real appended-session rebuild states: completed tool result, failed assistant with empty error and aborted assistant. Restored rows now use recorded completion times and frozen error defaults. `replay-frames-{red,green}.log` covers all15 states/30 frames at widths25/120. Pre-commit `cargo check -j3 --workspace --all-targets` passes (`replay-workspace.log`); strict TS/Biome/fmt/inventory pass; post-commit scoped clippy has100 existing diagnostics/0 changed-line findings (`replay-clippy*`). Readability review: three direct source-value corrections, pure fixture construction and one shared sequential frame-capture helper; no new API, carrier or dependency. This proof is scoped to Rust `d29e15875`; documentation-only changes preserve it.

## Thinking and queue-input checkpoint

Parent `67c697fcf` is integrated at `c58b08dd0`; its workspace/all-targets gate passes (`editor-parent-workspace.log`). Code `b6415483a` restores saved/live thinking visibility and status, combined queue clearing/order/spacing and dequeue messages, and the500ms Ctrl+C interval. Actual-source/native31 states compare62 complete width25/120 frames plus editor text, pending previews, settings and terminal progress. Genuine REDs: `editor-frames-red.log`, `editor-ctrl-c-red.log`; GREEN: `editor-frames-final.log`. Required pre-commit workspace/all-targets, strict TS, Biome, fmt and inventory pass (`editor-workspace.log`, `editor-typecheck.log`); scoped clippy96 existing/0 changed-line primary findings (`editor-clippy*`). Changed-line manual readability review is complete; ordered capture intentionally awaits each action, and queue transformation uses the existing lossless join. Locked inventory686/186679,636 done/25 gap/16 partial/9 todo. Whole mode, follow-up submission, Escape/cancellation and remaining transitions stay open. Documentation-only edits preserve this `b6415483a` Rust proof scope.

## Idle follow-up checkpoint

`78a718758` ports expanded-text follow-up routing and the existing default/custom editor submit callback authority. Two actual-mode receiver/queue cases (plain and real12-line bracketed paste) bring the corpus to33 states/66 width-specific frames. `follow-up-{red,green}.log` proves idle input reaches the main-loop receiver rather than the session follow-up queue. Required pre-commit workspace/all-targets, strict source TS/Biome/fmt/inventory pass (`follow-up-workspace.log`, `follow-up-typecheck.log`); post-commit clippy96 existing/0 changed-line findings (`follow-up-clippy*`). The source compaction/streaming branches and private helpers are implemented, but their real in-progress follow-up proof remains pending; no closure claim for those or whole mode. Readability review checks the three source routing branches, explicit helper effects and borrow-free callback invocation; no new trait/carrier/dependency. Rust proof scope `78a718758`; documentation-only edits preserve it.

## Next work

Continue realistic interactive command/event slices. Runtime's locked718-line audit is now [closed](traceability/agent-session-runtime.md#whole-module-audit-closure-2026-10-25), including optional initial SessionStartEvent and frozen public re-exports. The whole self-restart audit is closed with [existing unchanged proof](traceability/self-restart.md#whole-module-audit-closure-2026-10-25); do not rerun numeric/OS-fault matrices. Setup is no longer open. The apparent absent-model issue is resolved by frozen Agent's `initialState?.model ?? DEFAULT_MODEL` sentinel; no API change is required. No whole interactive-mode acceptance is claimed.
