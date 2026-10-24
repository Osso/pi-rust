# `components/footer.ts`

Authority: complete 271-line [common-pin](../source-lock.json) module. Rust: `crates/pi-coding-agent/src/modes/interactive/components/footer.rs` and shared `footer_format.rs`. [Contract](../../specs/interactive-footer.md).

## Coverage matrix — whole module

| Export / branch | Native behavior | Proof / state |
|---|---|---|
| source-private formatTokens | shared native JS number/rounding/toFixed helper | full real-source footer renders cover original 13 numeric vectors plus negative zero/negative count; no fabricated export |
| formatCwdForFooter | exact shared Node POSIX resolve/relative containment, raw JsString | 5 original exported-helper cases and 15 raw source path/home cases incl literal ~/ and U+0085 |
| constructor / setSession / override / clear / invalidate | cached session values; reset at exact source operations | mutable real main/child/second session managers compared against source class input services; append/name changes stay cached until invalidation; override changes and setSession invalidate |
| getSessionData | assistant-only sums; latest assistant cache-hit rate | actual assistant+tool usage RED; exact new sum output; main/child data and numeric fixtures |
| null override manager | no manager, no main name/usage leak | actual source RED; source fixture's former main-manager fallback removed; explicit null source/native comparisons |
| numeric truthiness | shared JsValue truthiness omits NaN totals/cost | all-NaN assistant source/native frames |
| cwd, labels, names | truthy strings only; HOME || USERPROFILE | empty-HOME source RED; raw U+D800 session name full render; branch/no-branch and override omission |
| context percentage/window | nullish defaults, exact fixed display and >70/>90 colors | 0/70/70.1/90/90.1/NaN/unknown, null model/context, original 25/75/95/unknown fixtures |
| model and thinking | empty ID -> no-model; empty effort string retained | real-source empty model/effort RED; full fixed frames |
| auto/experimental/provider/width | source indicator/styling/layout branches | auto on/off, experimental marker, provider fallback and widths180/80/40/12/1/0 |
| extension statuses | shared formatter, exact truncation | raw status + normal status and width transitions |
| dispose | Component's no-op instance method; free module API removed | append/name -> dispose leaves source/native cached frame unchanged; parent-authorized interactive cleanup line5992 calls actual footer instance |
| OAuth subscription (:175–178) | displayed-model provider reads live AuthStorage on each render; null model false; no Kimi special case | Actual frozen ModelRegistry.prototype.isUsingOAuth + AuthStorage.inMemory vs native session registry/storage; nine mutations/transitions with no cache invalidation: OAuth/API-key/remove, other/main/Kimi/null providers, setSession swaps storage |

## Corrected native/test discrepancies

The five RED counterexamples are native mismatches, not frozen source bugs: included tool-result usage, leaked main values for null override manager, NaN truthiness, empty model/effort substitution and empty HOME fallback. Native tests expecting leaked names/tool-inclusive totals were pre-repin contracts; the stats fixture now explicitly supplies the desired manager and asserts assistant-only 10k/2.0k, while the separate differential proves null-manager isolation.

The frozen class intentionally caches session data until explicit invalidation. That observed cache lifetime is retained and is not classified as a source bug. No new frozen bug was found in the non-auth branches. ContextUsage's unused tokens field need not be carried in the private cached rendering record; consumed contextWindow/percent remain exact.

## Source harness reconciliation

Old oracle code called non-exported formatTokens and silently supplied the main session manager for a missing override manager. Those alternate paths are deleted. Original numeric inputs now go through actual source renders; the exported cwd helper retains its direct proof. Four default-theme rendering cases and five cwd cases moved intact into a dedicated footer test, with actual frozen modelRegistry.isUsingOAuth and null override-manager semantics. The new deterministic source harness constructs the actual class with readonly session/provider fixtures; native uses real SessionManager/AgentSession fixtures, not copied rendering logic.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-footer-red-2.log`: five isolated real source/native mismatches. RED1 was only ambiguous .into test compilation, not behavioral evidence.
- `test-footer-green-2.log`: 1/1, 31 full rendered state sequences with real native sessions. GREEN1 was only the deleted old free-dispose callsite compile error; excluded.
- `test-footer-existing-diff-green-1.log`: 1/1, four original default-theme render variants plus five cwd cases.
- `test-footer-native-green-1.log`: native helper/render regressions4/4.
- `test-footer-raw-paths-green-1.log`: current source raw POSIX path differential1/1, 15 cases.

Seven prior focused tests cover the non-auth checkpoint. `footer-auth-red.log` reproduced missing subscription under an overridden displayed provider. `footer-auth-green.log` proves the live auth sequence plus all prior full-frame cases. Source registry method :826–828 reads actual credential.type; no test-copied subscription predicate. Native drops old-only Kimi shortcut and main-model heuristic. Whole footer module covered; parent owns final readability/clippy/independent gates and whole-mode/Q13 acceptance.


## Callable extension status boundary (2026-10-24)

FooterComponent.render_checked returns original CallbackError; its Component adapter uses the existing shared uncaught authority only for unobserved render failure. Actual frozen TUI.requestRender scheduling and doRender finally (tui.ts:962–1013,1897–1902) leave the original status callback TypeError uncaught; InteractiveMode uncaughtCrash remains the global terminal-restoring handler. Source process proof: status-callback-source-throw.log (same Error object, raw UTF16 message/code). Getter/render/callback-override/undefined retention and native process proofs are in tests/footer_callable_status.rs. [Contract](../../specs/extension-status.md).


The new typed throw path also requires the existing InteractiveMode crash policy to inspect original `.code` on NativeNodeError/CallbackError (including raw object reasons), not only io::Error. Actual frozen uncaughtCrash :5206 invokes isDeadTerminalError :337–342 before the already-shutting-down guard. Sixteen private-process cases exercise EIO/EPIPE/ENOTCONN/ESTATUS across native Error, callback Error, raw callback object and io Error. Genuine RED: status-callback-crash-policy-exact-child-red.log (nine typed cases source129/native1; old io cases unchanged). The fix changes only classification, not cleanup/lifecycle. Initial macro and incorrect private child-test module name were fixture errors, not behavioral REDs.

Final typed crash-policy16-case actual-source/private-process matrix GREEN: status-callback-crash-policy-green.log at25f9d2e9. Ordinary getter/render/Pyrun errors and scheduled original-throw forwarding are GREEN in status-callback-consumers-green.log; default-footer12/12 in status-callback-default-footer-green.log. No lifecycle/cleanup change.
