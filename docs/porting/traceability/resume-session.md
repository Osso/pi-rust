# Resume session — common-pin map

Entire `packages/coding-agent/src/core/tools/resume-session.ts` at immutable `4f9e6f8a4`; native `core/tools/resume_session.rs`. [Contract](../../specs/resume-session.md). Runtime switching and shared storage/DB implementations remain outside this slice; their existing APIs are used unchanged.

## Symbol and branch coverage

| Frozen symbol/branch | Native | Proof |
|---|---|---|
| Schema; input/details/params/resolvable-session records; factory metadata/prompts/sequential mode | JsString targets/name/path/starter, native records and exact ToolDefinition | Full enumerable descriptor differential; complete result comparisons |
| readNonEmptyString: absent, null/wrong/blank rejection, trim valid string | Native value validation and JS UTF16 trim; null is not absent | 39-case execution matrix covers field/target/starter priority, blanks, nulls, raw units and preserved trimmed starter |
| normalizeResumeSessionParams: object check, id/name/path validation order, exactly-one, starter only after target validation | Native record parser; JSON arrays use source exactly-one error, scalars/null use target-shape error | Same matrix, including actual array/null/number and target-before-starter histories; array behavioral RED before correction |
| assertResumeSessionPath: archive-aware read path, exists, separate fallible stat/isFile, suffix check, returned selected path | Existing read_session_transcript_path/is_archived_session_file; real stat with shared Node diagnostic authority; message-only ToolError boundary | Direct archive, absent-plain sidecar, suffix-only archive, wrong suffix, missing/file/directory and raw path cases; archived cwd consumer below |
| findUniqueSessionMatch: zero/one/multiple matches and original label | Native filtered references/count match | Missing/raw name, successful named lookup and ambiguous name; 11 retained behaviors |
| listLocalAndSessionDirSessions/listAllResolvableSessions: ordered scans and path-keyed last-value/first-position merge | Existing list/list_all plus explicit order vector/map | No/empty DB controls, normal ID/name resolution and default-root-only ID differential; ordinary scoped list duplicates must not cause ambiguity |
| resolvableSessionFromMetadata/sessionsInCurrentDirectory | Native owned id/name/path values; lexical resolve/prefix filter | Seven real persisted metadata histories; built-in callers only read the returned values, so approved owned-value policy applies |
| findMetadataSessions: name or ID lookup with optional DB | Existing native metadata queries | Scoped/global name/ID, optional/empty DB controls, stale metadata; no observed optional-DB output difference and no speculative shared change |
| findResumeSessionById: exact before prefix, duplicate-exact/prefix ambiguity, no match | Native exact/prefix matching | Current/unique/unknown/ambiguous IDs; scoped prefix beats global exact; existing exact-short-ID cwd consumer |
| resolveResumeSessionFile: path; scoped metadata/name, global metadata/name, scoped scans, all scans; ID delegation | Same ordered native stages | Seven scope histories: scoped-prefix/name, global-name, stale-scoped(no fallback), global-before-scan, scoped-prefix ambiguity and duplicate-exact-ID ambiguity; default-root-only ID; unknown name traverses the no-match scans |
| exported findSessionFileById | Shared tool helper, native string/path result | Execution ID histories plus real compressed archived-header cwd consumer; no changes to its caller's production code |
| assertResumeTargetIsDifferent | Fallible lexical path resolve against current path | Current path/ID/name rejection, retained behavior tests |
| formatResumeTarget/formatResumeSessionCall | path/id/name/target selection and optional warning marker | 14-frame source/native sequence: empty precedence, named target, whitespace starter marker, missing args, narrow width and raw UTF16 |
| formatResumeSessionResult | Shared text-only join, error/output style, empty string equivalent to source undefined??empty | Same sequence: mixed image/text/raw units, empty/error content, call/result reuse |
| execute: absent ctx/default rejecting handler, child truthiness, normalize/resolve/current guard, switch, optional replacement callback, cancelled/resumed result | Live typed ExtensionContext; always-present owned SwitchSessionOptions with optional WithSessionCallback; native terminating result | 39 execution cases,4 child controls, callback options-presence RED/GREEN,2 switch/starter raw-error histories and11 retained behaviors |
| renderCall/renderResult: reuse/new Text, setText failure | Existing fallible set_text_component; native TypeError, not replacement or panic | 14-frame differential and full descriptor; incompatible retained component cases for both renderers |

## Classified discrepancies and exclusions

| Case | Classification | Evidence |
|---|---|---|
| Any `.zst` suffix passes, while failure text names `.jsonl or .jsonl.zst` | DISCREPANCY, ported literally; main approval2026-10-24 | Frozen resume-session.ts:77-86 plus session-archive-storage.ts:7-10. Real `suffix-only.zst` accepts; wrong `.txt` uses exact frozen wording. Old-only `.jsonl` rule removed |
| Missing switchSession on a custom JS context | Typed-context exclusion, main approval2026-10-24 | Frozen runner.ts:946-949 always installs the method. Native stores a switch callback initialized to rejection, not an optional capability flag. None context still returns the frozen unavailable-mode error |
| Concurrent state changes between ordered async lookup stages or existence/stat probes | Schedule-dependent gap under recorded runtime policy | Literal ordering and fallible stat retained; no production timing hook, alternate path or independent producer scheduling emulation |
| Custom JS getters/proxies, invalid non-JSON runtime values and V8 stack text | Recorded typed/runtime exclusions | JSON validation branches actually represented by JsValue are covered; no synthetic stacks or object-reference aliasing |

The source always passes an options object, even without starter instructions. Native now passes Some(SwitchSessionOptions { with_session: None }); this is an observed callback argument value, not JavaScript object-identity emulation.

## Runtime evidence

- `resume-current-matrix-red.log`: seven actual source mismatches before0cd7d014 (archive selection/acceptance/wording and null targets/starter). Expanded39-case matrix GREEN `resume-array-validation-green.log`; optional/no DB controls also GREEN before array extension.
- `resume-child-guard-red.log` → `resume-child-guard-green.log`: empty ID alone is not a child; four controls preserved.
- `resume-render-red.log`: native silently replaced incompatible components; source TypeError. 7b421411 fixes via existing Text setter. `resume-shared-render-final.log` GREEN7 tests: resume/cwd/channel descriptors and14-frame histories, plus end-turn descriptor.
- `resume-metadata-scope-final.log` GREEN7 real metadata histories. `resume-default-root.log` GREEN actual default-root-only lookup, with all state inside Cargo's private test guard.
- `resume-options-value-red.log` → `resume-options-value-green.log`: source options always present versus native None without starter. Final GREEN2 also revalidates complete raw-error/switch/starter ordering after the test-only source recorder changes.
- `resume-retained-behavior-red.log` passed10/11; sole failure was the stale `.jsonl file` expectation. Exact frozen77-86 wording replaces it, with no relaxed assertion; targeted GREEN `resume-retained-behavior-green.log` completes that11-case proof.
- `resume-archived-cwd-consumer-green.log`: actual compressed bytes, metadata pointing at absent plain path, header cwd overriding stale metadata cwd, full source/native result and relocation; archive bytes and original metadata path remain unchanged. Initial consumer failure lacked the existing native worker fixture, not production archive support; corrected with support/transcript_worker only, no storage/runtime edits.
- Source callback recorder strict typecheck passes. General session-tools oracle retains baseline23 lint warnings and formatting debt; changed diagnostic/formatter spans are0, and the edited old assignment-expression error is removed. No broad unrelated fixture rewrite or suppressions. Coordination oracle is clean.
- Interrupted invocations lost captures; owned orphan trees were inspected/cancelled where present, then proof reruns used file-backed output. Exact commands/revision scopes and checkpoint gates: `/tmp/pi-rust-repin-core-tools/ledger.md`.
