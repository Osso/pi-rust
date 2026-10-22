# Node execution environment traceability

## Common-pin readTextLines re-port (2026-10-22)

Frozen `agent-core/src/harness/env/nodejs.ts:405–434` now maps to native `env/nodejs/read_lines.rs`, not whole-file read plus truncation. `ReadTextLinesOptions.max_lines` is f64: <=0 returns before I/O, positive fractions stop at the first integer line count >= limit, NaN/Infinity read to EOF. The shared option type and JSONL header reader caller use that same authority.

RED `node-lines-number-red.log` exposed the usize boundary; numeric GREEN `node-lines-number-green.log` covers 45 limit/line-ending combinations plus a missing-file negative-limit case. Two real FIFO differentials failed at the production lifetime boundary (`node-lines-{fifo,abort}-red.log`): a line limit and abort both incorrectly waited for writer EOF. Native buffered readline now stops/closes early and cancellation interrupts pending I/O with Node's exact error message. GREEN `node-lines-streaming-green.log`: three grouped tests. Tests release held FIFO writers and await native reads even on a failed assertion; no orphan work is left behind.

Whole NodeExecutionEnv comparison is still in progress. Old-only renameFile/inheritEnv and cleanup-kills-children behavior below are **not common-pin contracts** and are scheduled for removal; frozen cleanup is a no-op and several concrete methods ignore interface abort arguments. Do not treat the legacy table as current parity evidence.

## Common-pin repository ordering regression

Against `4f9e6f8a4` `agent-core/src/harness/env/nodejs.ts:486–504`, `differential_tree_repository` reproduces equal-timestamp JSONL listing order. RED: Rust returned raw directory enumeration order, unlike Node/libuv's native-name sorted `readdir`; `tree-repository-red2.log`. Fix: sort native directory entries before decoding/lstat, not repository metadata after enumeration. GREEN: whole memory/JSONL repository matrix and 23 `harness_env` tests (`tree-repository-green.log`). This is bounded current-pin evidence, not an upgrade of every old row below.

## Common-pin literal environment port (2026-10-22)

Frozen `packages/agent-core/src/harness/env/nodejs.ts` at `4f9e6f8a4` is authoritative. Legacy old-pin claims retired.

| Frozen lines | Observable contract | Proof/status |
|---|---|---|
| 47–49, 257–263 | Absolute paths returned literally; relative paths resolved, no tilde/file URL expansion; join keeps trailing slash | `differential_node_env::node_env_paths_match_frozen_posix_literals`, 12 cases |
| 223–229, 320–325 | Process → configured → per-call env; no inheritEnv option | `node_env_exec_environment_and_spawn_failures_match_frozen`, native override tests; old-only field deleted from exec and capture |
| 305–390 | Native spawn failure message, EOF waits for inherited stdio even after shell exit | missing cwd and delayed background-child stdout differentials; RED `node-literal-red2.log`, `node-stdio-red.log` |
| 567–569 | Cleanup is no-op, including while command executes | `node_env_cleanup_is_noop_and_exec_waits_for_inherited_pipes`; RED killed live child, GREEN retains `readydone` |
| 472–482, 511–565 | Concrete append/exists/create/remove/temp methods ignore extra interface abort args | `node_concrete_ignores_extra_abort_arguments`; existing native code already ignored them |
| 405–434 | readTextLines buffered line/abort/numeric domain | proof above, 45 vectors + actual FIFO lifetime cases |
| 392–567 | File kinds/read/write/list/canonical/errors/temp | retained `harness_env` 18 tests; exhaustive frozen filesystem failure comparison still pending |
| 34–44, 267–390 | Exec timeout, cancellation, callbacks, environment, shell/spawn | retained native tests; post-exit abort/timeout/late callback closure matrix pending |

GREEN `node-literal-green.log`: 4 direct frozen grouped tests + 18 native tests (unused test import subsequently removed). Source coverage row remains partial; shell configuration/failure and complete filesystem branches still require common-pin proof.

### DISCREPANCY / classifications

- Frozen cleanup does nothing; it does not terminate active children. Ported literally, not hardened.
- Frozen concrete methods without abort parameters ignore extra JS arguments despite the wider interface; retained literally.
- Background children retaining pipe descriptors delay settlement indefinitely unless they close or an active timeout/abort kills the process group. No grace cutoff is allowed; the finite held-pipe differential exposes delayed output.
- Old-only renameFile, inheritEnv, path expansion, custom missing-cwd guard and 100ms pipe grace removed, not kept as fallbacks.
- Windows-only discovery/taskkill/legacy WSL transport: pending user classification. Not ported or claimed.
- Temp UUID/name shape and full filesystem failure messages remain pending comparison, not approved deviations.
