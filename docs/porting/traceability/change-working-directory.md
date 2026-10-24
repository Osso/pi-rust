# Change working directory — common-pin map

Entire `packages/coding-agent/src/core/tools/change-working-directory.ts` at immutable `4f9e6f8a4`; native `core/tools/change_working_directory.rs`. [Contract](../../specs/change-working-directory.md). Actual relocation/activation/restart lifecycle remains runtime-owned; this module validates and schedules it without changing session identity.

## Symbol and branch coverage

| Frozen symbol/branch | Native representation | Proof |
|---|---|---|
| Schema; input/details records; factory metadata, prompts, sequential execution mode | Optional native UTF16 targets; exact JSON Schema; ToolDefinition | `differential_change_cwd_renderers::complete_definition_matches_frozen_schema_metadata_and_execution_mode` compares entire enumerable definition |
| readNonEmptyTarget: absent, wrong type/null, blank, unchanged valid string | Existing JsValue validation; null is not absent | 23-case execution matrix: missing/both/null fields/numeric/blank/whitespace/raw units; null-path behavioral RED before correction |
| normalizeTarget: object check; path before id; exactly one; trim id only | Native target enum; JsString trim | Same execution matrix; 9 behavioral tests; raw-unit normalization unit |
| assertExistingDirectory: exists then separate stat then isDirectory | OS existence check, fallible real `stat_directory_path`, exact Node native error fields; existing message-only ToolError at execute boundary | File/missing/NUL/symlink/raw-unit path histories; real ENOENT/ENOTDIR helper differential against frozen dependency wrapper; schedule gap below |
| loadTargetCwd path branch: resolve relative to ctx.cwd, directory check | Shared resolve_path; unchanged path whitespace/UTF16 | Absolute/relative/dot-parent/whitespace/symlink/NUL/raw existing/raw missing histories |
| loadTargetCwd id branch: shared lookup, current-file exclusion, header cwd, directory check | Shared resume-session lookup/read_session_header; original ID in details | ID/prefix/trimmed/raw/unknown/ambiguous/current-ID histories; metadata-selected missing-cwd header with no relocation; exact-short-ID behavioral test |
| formatCall: truthy path, else truthy id, else target; title/accent | Native JsString format and theme | 11-frame frozen sequence, raw D800 ID, empty path with ID, empty args, narrow width |
| formatResult: text-only filter/join, error/output styling, empty output | Shared exact tool_result_text/render_tool_output | Same sequence: mixed image/text, raw D800, error status, empty content |
| execute: absent ctx; child truthiness/short circuit; normalize, previous cwd, load, schedule; terminating content/details | Fallible live ExtensionContext getters; nonempty agent ID or required-ID flag; exact tool-call-ID scheduler | 4 child controls including empty-ID false; child guard RED before target validation; complete result/details and recorded relocation comparisons; 9 behavioral tests |
| renderCall/renderResult reuse Text or construct fresh; setText throw | Existing fallible render_utils::set_text_component; original native TypeError | 11-frame sequence verifies object reuse, reset, full frames, and incompatible prior component failures for both renderers. RED before routing correction |

Source's readonly contexts and native owned values follow the recorded typed/owned policies. `AbortSignal` and updates are intentionally unused in this source. The successful tool does not switch or mutate the referenced session; runtime delivery is tested separately by the existing call-ID behavioral cases.

## Explicit exclusions and gaps

| Branch | Classification | Evidence |
|---|---|---|
| Missing relocateAfterToolResult on custom JS contexts (source108-110) | Typed-context exclusion, accepted by main2026-10-24 | Frozen runner.ts:519-525 always installs the function; native ExtensionContext always provides the method. No new optional state or fallback |
| Directory changes between existsSync and statSync | Schedule-dependent gap, main ruling2026-10-24 | No production hook added to force timing. Literal two-call sequence retained; actual second-stat native errors preserve name/message/code/errno/syscall/path. Frozen dependency wrapper bypasses only exists; real stat supplies ENOENT/ENOTDIR, with zero relocation |
| JavaScript getters/proxies or wrong context/member runtime types; V8 stacks | Typed API/runtime policy exclusions | Native context has fixed typed members; no synthetic stack or JavaScript object/proxy emulation |

## Proof ledger

- `change-cwd-child-guard-red.log`: actual frozen child restriction vs native target-validation error. `change-cwd-child-guard-green.log`: corrected guard and retained seven-case matrix.
- `change-cwd-render-reuse-red-2.log`: actual frozen TypeError vs native silent replacement. Initial `-red.log` was test compile errors, not behavior. `change-cwd-render-reuse-green-2.log` GREEN11 frames; initial `-green.log` was callback ownership compile failure, corrected in abe449c9.
- `change-cwd-expanded-matrix-2.log` exposed a lossy test bridge (`ToolError::to_string`); source oracle returns error.message. Bridge now compares native message directly, without altering production or weakening assertions. `change-cwd-null-target-red.log` subsequently proves actual null target acceptance diverged; corrected23-case matrix plus4 child controls GREEN (`change-cwd-validation-green.log`).
- `change-cwd-stat-red{,-2}.log` are INVALID oracle evidence: attempted LD_PRELOAD fixture did not intercept Node's real backend; fixture removed. No stat code was changed from that evidence. Main required the literal separate stat and approved dependency-only wrapping instead.
- `change-cwd-stat-source-contract.log` GREEN2 real source stat failures with all original fields/zero relocations. `change-cwd-stat-native-api-red.log` fails the missing fallible helper API before implementation; `change-cwd-stat-native-green.log` compares that same production helper's real ENOENT/ENOTDIR against frozen whole errors.
- `change-cwd-header-contract.log` GREEN metadata-selected missing-cwd rejection and no scheduled relocation. `change-cwd-behavior-green.log` GREEN9 path/session/call-ID behavioral tests.
- `change-cwd-definition-render-green.log` GREEN complete descriptor and11-frame stateful renderer differential after parent integration. Checkpoint gates and exact revision/command scopes: `/tmp/pi-rust-repin-core-tools/ledger.md`.
