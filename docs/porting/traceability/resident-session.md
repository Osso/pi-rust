# resident-session.ts — frozen baseline with approved delta

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, `packages/coding-agent/src/core/resident-session.ts` (whole file read).

| Symbol / source branch | Native / targeted proof |
|---|---|
| `ResidentRole` | `ResidentRole::{Architect,Supervisor}` |
| `isResidentSession` exact IDs and slash/backslash complete segments | Existing predicate unchanged; prior `repin_session_storage` listing matrix retained |
| `prepareResidentSessionFile` recursive mkdir, regular-file filter, UTF-16 latest / canonical | `prepare_resident_session_file`; `repin_resident_session::source_created_trees_native_prepare_and_source_reader_reciprocal` |
| Live basename insertion, live/latest/canonical precedence | Paired actual source/native own-PID fixtures; missing file, unmatched file, directory basename |
| `findLiveResidentSessionPath`: null-agent role, missing listener/path, dead/non-Pi, resolved dirname, exact extension | Paired own-PID matrix; owned disposable source children for dead/non-Pi; relative lexical paths and symlink alias rejection |
| Metadata removal before unlink; immediate FS/DB failures | Actual-source mkdir/ancestor/scandir/unlink failure fixtures; invalid SQLite fixture |
| `isResidentSessionForRole`: role ID / path segment, missing path, exact live skip | Reciprocal metadata fixtures with other role, slash/backslash, prefix/suffix, symlink existence |

Dedicated oracle: `oracle/resident-session-oracle.ts`; tests: `crates/pi-coding-agent/tests/repin_resident_session.rs`. [Contract](../../specs/resident-session.md).

RED is an E0432 missing-API compiler failure: **no runtime assertions reached**. Source-only error probes confirmed EEXIST/mkdir, ENOTDIR/mkdir, EACCES/scandir and EACCES/unlink under the mandatory guard before implementation. First full targeted run reached runtime assertions: FS/DB cases passed, metadata matrices failed on the private dynamic-RegExp backslash discrepancy. No whole-module promotion yet. Eight independent behavior mutants were killed against passing selectors/error fixtures (not against known RED); exact substitutions/logs in `/tmp/pi-rust-repin-resident-session/mutations.json`.

Native and TS self-PID branches intentionally use different actual processes; no fixture claims to impersonate a full Pi runtime. Disposable dead/non-Pi children are owned; no foreign PID signaling, services or live-state cleanup. No consumer wiring required by this source module.

Local POSIX Node lexical join/resolve preserve leading relative `..`: shared `utils::paths::node_join` drops those components and `node_resolve` does not anchor relative paths to cwd. Shared NodeFs IO error record formatting is reused; an actual owned-file EEXIST error exposed pre-existing VT inspection `\\v` versus Node `\\x0B`. Parent authorized the one root escape fix, covered by `pi-js-compat/tests/node_fs_errors::actual_vertical_tab_filesystem_error_console_matches_node` (runtime RED first). No context-specific compatibility callback remains. Following actual-source NUL-path runtime RED, parent authorized one minimal shared `pi-js-compat/src/node_fs.rs` path-argument error helper; its TypeError record has no invented IO fields, and messages use `JsString` to preserve UTF-16 truncation. Source fixtures cover quotes, controls, multiline, >10000-unit inputs and split-surrogate excerpts.

Approved 2026-10-21: fix the TS private role-specific dynamic RegExp to recognize both separators and port that explicit delta; keep the frozen reference unchanged. Actual frozen source retains missing backslash-only role metadata while native removes it (public literal predicate already recognizes both). Exact fixtures remain in the three source/native metadata matrices; the separate TS-fix pin is recorded in [the module delta record](../deltas/resident-session-private-regex.json). Frozen-only checkpoint `7eee2fc8981839536fd31ca81ef3fcb1e08aa0cc`: four tests pass and three full-snapshot matrices fail after all cases execute. All 42 snapshot mismatches are exactly one frozen-retained backslash-only missing row each, no other differing rows. Independent file/path/liveness selection covers both roles × 23 cases; NUL validation covers 11 cases; actual filesystem errors cover seven cases. Shared NodeFs suite is 6/6 GREEN, including actual owned VT-path errors. Frozen-only parity remains RED, not misrepresented as whole-module GREEN.

Approved 2026-10-21: Unix-only new preparation API/helpers, enforced by `cfg(unix)`. Existing public predicate remains cross-platform and unchanged. Windows preparation is an explicit gap; Linux runtime proof does not assert macOS execution.


## Explicit post-pin delta oracle

No shared post-pin delta ledger existed in `docs/porting`; the minimal mechanism
is one module-scoped [JSON pin](../deltas/resident-session-private-regex.json).
It records approved TS commit `8f8f3c2621a595ff799cd42cb4d14399cd1e286d`
(parent is the frozen pin), checkout, alias, module and SHA256
`3397ada41e973140e5230c2aae2c152ba72696d2def7a3b216528c2e35ab1ade`.
The private dynamic-RegExp escape fix is the only production delta; public
predicate and common source lock are unchanged.

`repin_resident_session::approved_delta_oracle` verifies checkout HEAD and module
SHA256 before spawning tsx. Missing checkout/module, wrong HEAD/hash or import
failure is fatal; there is no retry against frozen source. Only the resident
module import is selected; the oracle's DB reader and tsx still use the frozen
lock. Frozen entrypoints explicitly clear the resident selection environment.

Four **explicitly ignored opt-in delta tests** reuse the unchanged reciprocal,
paired-own-PID, owned-dead/non-Pi and independent-selection matrix bodies.
Their ignore reason names the runner and invocation. The original seven frozen
tests remain **unignored**, including all three intentionally RED metadata
matrices: classification **frozen-baseline-with-approved-delta**. Their 42-row
historical discrepancy artifact remains intact, not converted to expected-pass
assertions or counted as delta proof.

Reproduction (Cargo outside guard; use the host target triple if different):

```text
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=$PWD/target CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="python3 $PWD/scripts/resident-delta-runner.py" cargo test -p pi-coding-agent --test repin_resident_session delta_ -- --ignored --test-threads=1
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=$PWD/target cargo test -p pi-coding-agent --test repin_resident_session resident_delta_selector -- --test-threads=1
```

The test-only runner outer-mounts the approved checkout read-only at
`/opt/pi-approved-deltas`, then invokes mandatory `scripts/test-sandbox.py`.
Only a fresh private proof directory is writable. This explicit alias is needed
because the mandatory guard hides `/home`; no common guard/runner configuration
changes, live state or source writes. Rust verifies the alias's actual worktree
HEAD through the common read-only Git metadata already exposed by the guard.

Targeted RED before selector implementation: reciprocal delta entrypoint still
using frozen source failed with 14 backslash metadata differences, all 16 cells
executed. Three integrity tests also failed against a permissive test selector.
Positive delta proof is pending the focused post-commit invocation; no broader
gate or cross-platform acceptance is implied.
