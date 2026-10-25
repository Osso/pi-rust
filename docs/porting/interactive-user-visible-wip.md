# Interactive user-visible parity — active successor

## Authorized scope

User-observable `interactive-mode.ts` startup/render/events and command/session-transition slices, `agent-session-runtime.ts`, `self-restart.ts` and `keybindings.ts`, against common pin `4f9e6f8a4`. Keep existing lossless carriers. Mechanical cross-lane adapters are cleared; semantic deviations/dependencies/TS deltas still need approval. No agents, full-suite runs, push, merge, deploy or live providers. Parent owns integration and independent verification.

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

## Next work

Continue realistic interactive command/event slices. The whole self-restart audit can reuse unchanged `7fd6dd94` process evidence (only include-path and a needless-borrow cleanup differ); do not rerun numeric/OS-fault matrices. Runtime callback setup still needs a final realistic API review before whole-module closure. No whole-mode/runtime acceptance is claimed.
