# resident-session.ts — frozen baseline with approved delta

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, whole `packages/coding-agent/src/core/resident-session.ts` read. Approved 2026-10-21: fix only the private dynamic-RegExp template's backslash escaping; public predicate unchanged. TS delta `8f8f3c2621a595ff799cd42cb4d14399cd1e286d` has the frozen pin as direct parent. Its immutable checkout and module SHA256 are in [delta-lock.json](../delta-lock.json). [Contract](../../specs/resident-session.md).

| Symbol / branch | Native / concrete proof |
|---|---|
| `ResidentRole` | Native architect/supervisor variants; every role-dependent fixture runs both |
| `isResidentSession` exact IDs and slash/backslash complete segments | Existing predicate unchanged; prior `repin_session_storage` listing matrix retained |
| `prepareResidentSessionFile` recursive mkdir, regular-file filter, UTF-16 latest/canonical, lexical join | Reciprocal files/metadata matrix, both roles × 8 cases; Unicode U+10000 versus U+E000, symlink candidates, relative parent/dot paths |
| Live basename insertion and precedence | Paired own-PID full snapshots, both roles × 13 cases: missing/unmatched/directory/symlink basename, lexical relative path |
| `findLiveResidentSessionPath` | Null-agent role; absent/empty path; exact resolved dirname/extension; child-agent exclusion; owned dead/non-Pi children |
| Metadata removal before unlink; immediate errors | Seven actual-source FS errors, protected unlink ordering and invalid SQLite; no invented errno/syscall on NUL argument errors |
| Private role predicate; missing-only metadata cleanup and exact-live exemption | Full snapshots contain both separators, role-ID-only, other role, prefix/suffix lookalikes, existing/broken symlinks; frozen exact-difference contract and delta zero-difference contract |

## Oracle selection and frozen evidence

`oracle/resident-session-oracle.ts` uses frozen source by default. Only `approved_delta_oracle` selects the resident module from the tracked delta lock; it verifies HEAD and module SHA256 before spawning. Common tsx and the oracle's DB reader still use the frozen lock. Frozen entrypoints explicitly clear the resident-module selection environment. No global retargeting, source rewrite or fallback.

The mandatory [sandbox](../oracle-gate.md#locked-post-pin-deltas) validates and exposes locked delta checkouts/Git metadata read-only at their actual paths. The old ad-hoc `/opt` alias runner is removed. Both source selections run in ordinary Cargo gates, with **no ignore attributes or special runner override**:

```text
CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=$PWD/target cargo test -p pi-coding-agent --test repin_resident_session -- --test-threads=1
python3 scripts/test-sandbox.py -- python3 scripts/test_test_sandbox.py
```

Classification: **frozen-baseline-with-approved-delta**. Frozen tests named `frozen_approved_delta_8f8f3c26_*` compare observed differences against the exact tracked [42-row record](../deltas/resident-session-frozen-discrepancies.json), partitioned into reciprocal (14), own-PID (26) and owned-child (2) matrices. Match keys are `case`, `sourceOnly`, `nativeOnly`; every source-only row is the frozen-retained missing backslash role path. Missing, extra or altered differences fail. Nonmetadata snapshots and common metadata order/count must also match exactly, so an approved row cannot conceal a file/path/order regression. Delta matrices require an empty difference set. Neither mode waives assertions.

Original historical RED artifact remains unchanged: `/tmp/pi-rust-repin-resident-session/frozen-discrepancies.json`, SHA256 `2c4c3079c68f8c4ddc22c4b75141738b8a2ebe89e31b3534cc7c204c60b28438`. Its 42 case/sourceOnly/nativeOnly values were re-observed at `d7371ee6` before conversion to the exact expected-difference assertions. That original RED is discrepancy evidence, not delta failure.

## Runtime coverage and proof levels

| Matrix | Concrete coverage | Proof |
|---|---|---|
| Delta reciprocal files/metadata | Both roles × 8 complete snapshots | 16 comparisons, zero mismatches at `62ed12e8` |
| Delta paired own-PID | Both roles × 13 complete snapshots | 26 comparisons, zero mismatches at `62ed12e8` |
| Delta owned dead/non-Pi children | Supervisor × 2 complete snapshots | 2 comparisons, zero mismatches at `62ed12e8` |
| Independent selection/liveness | Both roles × 23 cases; paths/files/types and missing-live metadata | 46 cells, zero mismatches at `62ed12e8` |
| Selector integrity | Missing checkout, wrong HEAD, wrong module SHA256 | 3 tests GREEN in ordinary target at `62ed12e8` |

Ordinary mandatory-runner proof at `62ed12e8`: **20 tests pass, zero fail, zero ignored**. Includes all four delta matrices (zero differences), all three frozen matrices (exactly the 42 recorded differences), six comparator regressions, three selector integrity regressions, existing 46-cell frozen selection, 11-case NUL, seven filesystem errors and invalid DB. The historical alias-runner positive proof at `d7371ee6` is superseded for runtime integration, not discarded.

Sandbox proof at `8aeec134`: **10 tests pass**. Covers five actual Git/hash/absence validations, actual read-only locked-checkout visibility, nested entropy/devices, installed standalone Python, ripgrep isolation and private state/loopback-only networking. The prior 9/10 run exposed a test assumption: a bind-mounted launcher loses its symlink identity, so the test now selects the installed standalone Python from the launcher's actual shebang. No guard isolation was relaxed. Only this test changed after resident GREEN; resident proof remains valid.

Logs: `/tmp/pi-rust-repin-resident-session/ordinary-resident-green.log`, `ordinary-sandbox-green-final.log`. No broad workspace acceptance, clippy/check or new TS type-gate claim.

TDD: initial missing-API RED was E0432, **no runtime assertion reached**. Later snapshot RED exposed the source regex discrepancy. New delta reciprocal entrypoint before selection failed on exactly 14 rows; selector tests failed before validation. Exact-frozen comparator tests failed before implementation for missing/extra/altered differences, common-row reordering, file changes and required difference preservation. Sandbox lock regressions failed before validation on absent checkout, wrong HEAD, changed/missing module and verified-path exposure. Logs/commands: `/tmp/pi-rust-repin-resident-session/{REPORT.md,ledger.md}`.

Eight original runtime mutants were killed against previously GREEN selectors/error fixtures; no compile error counted. Exact substitutions: `mutations.json` in that proof directory. Disposable dead/non-Pi children are owned; paired self-PIDs are different real processes, not impersonated Pi runtimes. No live-state or consumer-wiring claim.

## Error boundaries and limits

Local POSIX lexical join/resolve preserve relative `..`; no shared path refactor. Source-proven NUL validation uses a minimal shared `pi-js-compat` TypeError helper, with UTF-16 quotes/control/multiline/truncation and no IO fields. An owned-file VT EEXIST regression exposed the shared `\\v` versus Node `\\x0B` inspector root; one authorized shared escape fix, no context callback. NUL 11-case and filesystem 7-case proofs reran GREEN in the ordinary resident target. Shared NodeFs 6-test proof remains valid on unchanged implementation; no redundant rerun.

Approved 2026-10-21: Unix-only preparation API/helpers (`cfg(unix)`); existing public predicate stays cross-platform. Windows preparation is a gap; Linux proof is not macOS execution. No clippy/check, broad suite, final acceptance, delegation, push, merge, deployment or live services in this continuation.

## Runtime-services revalidation (2026-10-25)

Whole module re-read against common pin and retained approved private-regex delta. `cargo test -j3 -p pi-coding-agent --test suite_session_1 repin_resident_session`: 20 pass, zero ignored at 567abb2c7; `/tmp/runtime-services-green-resident.log`. Includes frozen exact discrepancy matrices, approved delta reciprocal matrices and actual filesystem/listener boundaries. No product code changed in this revalidation.
