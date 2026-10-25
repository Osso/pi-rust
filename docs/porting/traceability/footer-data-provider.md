# Footer data provider

Common-pin [source lock](../source-lock.json): `coding-agent/src/core/footer-data-provider.ts`456lines. [Contract](../../specs/footer-data-provider.md); [readonly prerequisite](readonly-session-manager.md). Native implementation: `crates/pi-coding-agent-tools/src/core/footer_data_provider.rs`. Whole-module native revalidation is complete within the explicit boundaries below; [checkpoint proof](../footer-data-provider-proof.md) records current scoped gates. The older450-line/type-erased audit is superseded.

## Complete export/branch matrix

| Frozen surface | Native representation/behavior | Proof level |
|---|---|---|
| GitPaths; findGitPaths31–63 | Raw JsString repo/common/HEAD paths; Node lexical helpers and actual OS conversion only at I/O. Regular/worktree/commondir; malformed file/FIFO/socket/broken link walk upward; missing HEAD/worktree HEAD/stat/read failure stop | Existing regular/worktree tests + raw high/low/pair paths;6 actual-source metadata-kind cases RED→GREEN ate648a21ba |
| resolveBranchWithGitSync66–74 / Async77–96 | Fixed git argv/cwd; replacement UTF8 then JS trim (not U+0085);1MiB stdout limit; sync stdin/stderr ignored, async pipes/stderr limit and TERM-on-overflow | Actual-source real fake-git7-case process RED→GREEN at14bfd370d: trim, invalid UTF8, stdout/stderr overflow, empty, nonzero, missing executable |
| isWslEnvironment98–100; mount regex102–108 | Linux and nonempty WSL variable; entire `/mnt/[a-z]` prefix is ASCII case-insensitive with slash/end boundary |3 actual-source/private-process real metadata cases RED→GREEN at485dc7942: empty env, ctime-only chmod, uppercase prefix; no host WSL/mount enablement |
| normalize/resolveFooterExecutableName110–127; getExecutableName170–172 | Configured valid pi-* name wins; otherwise argv1/execPath basenames; empty/default pi/non-pi omitted; raw units retained | Legacy executable matrix plus actual-source raw executable/path cases in raw_footer_statuses |
| constructor155–159; getGitBranch162–167 | Discover/watch immediately; branch read cached lazily, null differs from uncomputed; ref branch, `.invalid` git lookup/detached, non-ref detached, read error null | Real regular/worktree/reftable tests;7 process scenarios; source refresh unread/same/read-failure cases |
| ExtensionStatus25; getExtensionStatuses175–184; set/clear208–219 | Ordered JsString/Text/Callback carriers; evaluate each read with current override, omit undefined without deleting carrier; propagate original Result errors | raw_footer_statuses, footer_callable_status and existing checked/render/Pyrun/process consumer proofs; current real render/Pyrun/no-op/default consumers GREEN |
| onBranchChange187–190; notify259–261 | Ordered live registration IDs; additions/removals/disposal during notification take effect immediately. Callbacks return Result with original error | Actual-source first/added/added vs former first/removed/added RED; live/dispose and synchronous TypeError/code/raw-unit2-case GREEN at4b5eb8613 |
| FooterSessionOverride16–22; get/set/clear192–205 | Owned display data; live [readonly session view](readonly-session-manager.md), not mutable SessionManager mutex | Missing API/override compile RED→GREEN; source/native isolated reads/live goal writes2 cases; 128-descendant/exact-error wrapper and current real-render consumers GREEN |
| get/setAvailableProviderCount222–229 | Full f64 storage (atomic bits), including fraction/negative/nonfinite/-0; mechanical renderer/interactive/Pyrun adapters | Source/native7-number domain compile RED→GREEN at3a53efa36 |
| setCwd231–246 | Same-cwd no-op; cancel debounce/clear watchers/reset cache and metadata/reinstall/notify; original notification error reaches caller | Legacy recomputation test + source live callbacks + refresh cwd-in-flight case |
| dispose249–257 | Mark disposed; cancel debounce/watch/poll/retry; clear callbacks; an already-running git completion is suppressed | Source retry/dispose + dispose-in-flight and real watcher lifetime scenarios |
| scheduleRefresh263–273; refresh275–299 |500ms debounce, pending coalescing, lazy/no-change notification suppression, error-finally cleanup and one pending reschedule | Actual-source/real-watcher7-scenario proof at66699561b,28.16s, including original callback error recovery; post-extraction4-test/14-scenario lib target GREEN29.48s |
| resolveGitBranchSync301–313 / Async315–329 | Raw HEAD read/JS trim; exact prefix; `.invalid` branch helper or detached; source catch maps I/O failure to null | Real metadata/source refresh read-failure and fake-git process matrices |
| clear/retry/error331–363 | Close/drop HEAD, poll, reftable/list; cancel retry;5s retry, no retry when disposed/already scheduled | Real EACCES setup failures + runtime-only injected error→clear→5s retry→new live branch→dispose source/native proof GREEN13.51s atf7ec04d8f |
| setupGitWatcher365–376; HEAD378–396 | Watch HEAD directory (atomic replacement survives); missing filename/HEAD trigger refresh; failed HEAD watch stops dependent watches unless WSL polling enabled | Real-watch3-scenario RED→GREEN at22bcb488e; current System constructor path is exercised, not mocked |
| HEAD poll398–415 | WSL Windows-mounted HEAD only,1s interval, Node-style binary64 mtimeMs/ctimeMs/size; no tables.list polling | Real metadata/process3-case proof above; old native birthtime/raw-nanos/presence-only behavior removed |
| Reftable417–444 | Watch common reftable directory and existing tables.list; failed directory watch stops list installation | Real chmod111 directory EACCES with accessible list exposed old incorrect branch refresh; RED→GREEN. One list mutation across1s boundary exposed absent-source extra poll |
| ReadonlyFooterDataProvider448–456 | Six getter/subscription methods form the native read projection; no separate trait/runtime wrapper | Parent2026-10-24 approves this type-projection boundary: frozen Pick creates no runtime wrapper, so a native trait is unnecessary |

## Real faults and bounded injection

Real private experiments: removing a watched directory emits rename twice; chmod on an active watcher emits rename, not error. New watches on execute-only directories fail EACCES. Private `/proc/self/fd/2` and `/proc/self/mountinfo` remain watchable. Evidence: `footer-real-watch-fault-experiments.log`.

Setup-failure proofs therefore use actual chmod111 errors and real inotify delivery, not a mock. The only injected failure is an already-active watcher's runtime error event, which those real triggers could not produce. Hidden `with_git_and_watch_factory` is footer-only test dependency injection with drop-only handles and close-aware callbacks. Normal constructors always select `watch_system_path`; no test-mode behavior branch, shared watcher change or dependency is added. Post-extraction System watcher integration passes alongside the other footer lib scenarios.

The source fixtures replace only child-process backend functions to control git completion/output for watcher-ordering scenarios. Separate fake-git process tests exercise the real source and native process backends and exact OS argv/stdin/stdout/stderr/cap behavior.

## Shared process prerequisite

Parent approved the minimal explicit kill-signal option on shared async capture. Actual Node execFile overflow runs a Python SIGTERM trap with stdout already closed (EPIPE); the former native SIGKILL path produced no marker (genuine RED). Current TERM plus explicit KILL regression passes atd516ec738. Readers are destroyed/awaited before signaling/reaping; existing callers pass SIGKILL explicitly, footer uses SIGTERM. No alternate escalation/sink or package/version change.

## Discrepancies and approved boundaries

| Item | Classification |
|---|---|
| Old lookup completes after setCwd | DISCREPANCY, preserved by parent2026-10-24: old-repo lookup can overwrite the new cache and notify the stale branch before a pending refresh restores it. Pinned trace direct→next→direct; no generation guard/delta |
| Impure status callback mutates map | Existing approved gap: native reads a carrier snapshot rather than JS live-Map iteration. Frozen25 requires pure callbacks; no lock is held across them. This neither caches output nor freezes current display data |
| Custom structural manager inputs | Approved typed-context exclusion in the [readonly map](readonly-session-manager.md#approved-typed-input-boundary); no built-in consumer passes one |
| ReadonlyFooterDataProvider Pick | Approved type-projection boundary above, not a runtime behavior gap |
| JS object identity / returned reference mutation | General owned-value approval; built-in consumers use explicit native mutations. Callback listener mutation relied on by consumers is implemented explicitly, not waived |
| Platform proof | Real kernel watcher proofs run on Linux; no macOS kernel proof is claimed. Windows-only runtime exclusions follow AGENTS. WSL detection is tested without enabling WSL |

## Proof status

Current command/revision/invalidation ledger and logs: `/tmp/pi-rust-repin-lossless-interactive/footer-provider-proof-ledger.md`. Current bounded runtime70 distinct cases pass, including the actual scheduled-callback fatal process and128-descendant/exact-error wrapper tests. Strict five-oracle ES2024 no-emit/Biome, format and workspace0/0 pass; composite all-target/all-feature clippy has0 changed-hunk findings with1677 outside-owned warnings. The sole later code edit removes a needless borrow and is covered by focused target recompilation. Manual Rust/TS readability audit extracted watcher/publish stages; metric executables are unavailable, so no automatic metric claim. See the checkpoint proof for exact revisions/scopes and inventory. No ignored tests, relaxed assertions, broad project suite, live host mutation, agents, push or deployment.
