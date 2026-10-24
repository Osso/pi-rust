# Node execution environment — common-pin traceability

Frozen: `packages/agent-core/src/harness/env/nodejs.ts`, immutable `4f9e6f8a4`. Native: `crates/pi-agent-core/src/harness/env/nodejs.rs` and `nodejs/read_lines.rs`. Status **complete with approved Windows-platform gaps**. POSIX and unguarded shell-path behavior have common-pin proof; no old-pin proof upgrades this row.

The existing readline `LineBuffer` now lives unchanged in [pi-js-compat](../../specs/node-readline.md), shared with the FD tool. File reader early-limit return calls finish only after a completed line (pending bytes empty), preserving the previous direct lines return. `repin_node_lines` keeps the45 limit/ending combinations and FIFO early-close/abort boundaries GREEN (`find-shared-readline-regression.log`). No file-reader algorithm/lifetime change.

## Assigned tmpdir authority correction

Frozen `nodejs.ts:548` explicitly invokes `mkdtemp(join(tmpdir(), prefix))`; `createTempFile` calls it at555. Only this native base-directory getter is routed through shared [POSIX tmpdir](../../specs/node-tmpdir.md). `differential_tmpdir_consumers::node_environment_uses_mutable_node_tmpdir_for_directories_and_files` compares actual frozen/native created-directory parent, empty-file parent/bytes and random-name shape across TMPDIR/TMP/TEMP/trailing-slash mutations. `tmpdir-node-red.log` proves the previous std getter ignored those mutations. Other filesystem behavior is unchanged; GREEN follows the coherent correction commit.

## Export/branch matrix

| Frozen lines | Contract | Proof |
|---|---|---|
| 246–255 | `NodeExecutionEnv` constructor cwd, optional shellPath/ProcessEnv | Native raw-shell/env differential; configured undefined values omitted |
| 47–49, 257–263 | Absolute path literal, relative resolve, native join; no tilde/file URL expansion | `differential_node_env::node_env_paths_match_frozen_posix_literals`,12 cases; raw-byte cwd subprocess compared with Node process.cwd/path.resolve |
| 34–44, 265–390 | Exec timeout domain, preabort, cwd/shell/spawn, streaming callbacks, abort/timeout/callback precedence, exit code |18 retained `harness_env` tests; source/native post-exit timeout/abort/native callback matrix; exact spawn validation8cases |
| 109–159, 178–211 | POSIX shell discovery and fallback | `repin_node_discovery`:6 cases in private mount+PID namespaces with /bin/bash hidden; found/nonzero/bad-path/trailing-space/BOM/5s-hung which |
| 213–219, 320–325 | Process→configured→per-call env, integer-key enumeration | Native shell/env and8case spawn matrix; names String per parent-confirmed env rule, values JsString; no inheritEnv |
| 221–243 | Group kill then positive-pid fallback | Actual nongroup discovery child timeout RED/GREEN; timeout/abort shell tests |
| 51–103, 477–484 | Kinds, signed mtime, FileError classification/message vs outer native path |13case filesystem read matrix: FIFO, preepoch, dangling/missing/ENOTDIR/NUL/unmatched units/EISDIR/open/realpath/scandir |
| 394–403, 436–445 | Text/binary reads, preabort and in-flight abort | Real FIFO source/native cancellation after IO started; EOF peer release awaited |
| 405–434 | Buffered readLines, f64 limit, streaming abort | `repin_node_lines`:45numeric/line-ending vectors, negative missing-file no-IO,2 held-FIFO lifetime cases |
| 447–475 | Write/append parent mkdir, raw text bytes, failure outer target path |13mutation-operation matrix plus native tests; blocked-open write cancellation asserts both persisted bytes and aborted result |
| 486–508 | Sorted listDir, per-entry lstat, invalid-kind skip, abort | Native symlink/listing/preabort tests; repository equal-time ordering RED/GREEN before metadata parsing |
| 510–544 | Canonical, exists, create/remove options |13read+13mutation matrices; dangling symlink, unsupported FIFO, missing/force/recursive/symlink/directory cases |
| 546–565 | Temp directory prefix, UUIDv4 file name, empty file, failure paths | Temp success and3failure source/native cases; UUIDv4 and six-alphanumeric suffix shapes asserted; only random identities normalized |
| 566–568 | Cleanup no-op | During active command, frozen/native retain readydone; inherited background stdio waits to EOF and retains earlylate |

## RED/GREEN ledger

Artifacts: `/tmp/pi-rust-repin-delta-agent-core/ledger.md` and logs in that directory. Each checkpoint also has core/coding `cargo check --all-targets` logs; formatting passed before every commit.

| Correction | RED | GREEN |
|---|---|---|
| Raw Tool label/description carriers (separate types slice) | tool-native-red.log | tool-native-green.log, tool-coding-green2.log |
| Literal paths, cleanup, native spawn cwd, no pipe grace | node-literal-red2.log, node-stdio-red.log | node-literal-green.log,4frozen groups+18native |
| Post-exit timeout/abort/callback stay active through pipe EOF | node-postexit-red.log | node-postexit-green.log,5groups+18native |
| Shared OS path decoding | node-path-decoder-red.log | node-path-pi-js-compat-green.log,3byte vectors; node-path-process-cwd-green.log,isolated malformed-byte cwd |
| Signed metadata, FIFO exists, NUL/native diagnostics and EISDIR read | node-fs-matrix-red2.log | node-fs-matrix-green.log,7groups+18native |
| Write/append mkdir failure reports target outer path | node-fs-mutation-red.log | node-fs-mutation-green.log,13operations+actual bytes/state |
| Temp UUID and returned mkdtemp path repair | node-temp-red.log | node-temp-green.log |
| Native shell/env carriers | node-shell-native-red.log | node-shell-callers-green2.log,10env groups+1capture matrix+18env+7capture native |
| Node spawn validation/enumeration | node-spawn-validation-red.log | node-spawn-validation-green.log,8cases; node-spawn-extension-callers-green.log,2extension-editor process groups |
| In-flight read/write cancellation | node-read-abort-red.log,node-write-abort-red.log | node-io-abort-green.log,13env groups+18native+3readline groups |
| Exact POSIX discovery parsing/pid kill fallback | node-discovery-red4.log,node-discovery-bom-red.log,node-discovery-timeout-red.log | node-discovery-green.log,6cases/11.48s |
| Temp failure syscall/prefix validation/omitted outer path | node-temp-error-red2.log | node-temp-error-green.log,3cases |

Raw-byte cwd proof uses direct Node: tsx/esbuild itself cannot spawn from a UTF-8-repaired-but-nonexistent cwd. The isolated native child compares shared `node_fs::decode_node_path` with actual Node process.cwd and path.resolve; no frozen-module claim depends on the failed tsx setup.

## DISCREPANCY / deliberate literal behavior

- Cleanup does not kill active children. Background children retaining descriptors can block settlement indefinitely; no grace cutoff is allowed.
- Concrete append/exists/create/remove/temp methods ignore extra interface abort args. Their ignored-argument source behavior is retained, not hardened.
- Cancellation during a blocked file-write open can still persist hello before returning aborted. The differential asserts both effects.
- Failed createTempFile leaves its created temporary directory behind. Tests observe the failure path and explicitly remove that directory afterward.
- listDir reconstructs decoded entry names before lstat; unsupported kinds are skipped. Getter/reference behavior follows the approved general owned-value rule.

## Pending/excluded

- Rejection carrier and required API fixture adapters are integrated and proven below.
- Windows-platform branches are approved gaps (AGENTS.md2026-10-22), classified below. The unguarded legacy WSL-path/stdin branch is implemented and proven on POSIX, not waived.
- Raw OS path ingress (cwd, readdir names, realpath and tmpdir) uses the shared node_fs decoder. A full crates/pi-agent-core grep finds no remaining to_string_lossy sites.

Removed old-only renameFile, inheritEnv, path expansion, custom missing-cwd guard, child tracking and100ms stdio grace. No compatibility fallback retained.

## Native IO rejection carrier

Parent authorized2026-10-22. `OperationFailure<E>::Returned(E)` means the frozen promise resolved to err(E); `Rejected(NodeRejection)` means it rejected with the original native Node error. NodeRejection retains name/code/errno/syscall/message. Only FS/Shell methods whose frozen resolvePath is outside try gain that carrier; join/temp methods keep their existing returned-result APIs. The cwd expect panic is removed.

`node-rejection-red.log` reproduces the real deleted-cwd panic in an isolated process. `node-rejection-green.log` compares raw Node uv_cwd ENOENT properties. `node-rejection-callers-green2.log` compares18source/native outcomes:12resolving FS methods, exec with requested cwd, join (still succeeds), prompt/skill loaders, JSONL repo and shell capture. Prompt/skill/JSONL callers preserve rejection rather than generate diagnostics or a prefixed SessionError. JSONL repository operations and session writes expose SessionFailure; pure session getters and memory-repo APIs are unchanged. Frozen shell capture has its own try/catch: it returns unknown ExecutionError with the original native Node cause, not a rejection. The test asserts that distinction explicitly.

Legacy fixture adaptation preserves all existing assertions and now explicitly fails on an unexpected rejected promise before inspecting a returned backend error. `node-rejection-adapters-development4.log`:50 focused caller tests GREEN across11targets, including frozen resource/session/repository/capture matrices and private POSIX discovery. This is bounded development proof, not the final full package suite.

OS path authority follow-up: `raw_os_filename_and_realpath_bytes_match_node_repair_boundary` uses a real filename containing FF and ED A0 80 bytes plus an ASCII symlink. Frozen realpath returns a UTF-8-repaired path; frozen listDir reconstructs that repaired name and fails lstat although the original file exists (DISCREPANCY retained). Characterization was already GREEN before the mechanical helper routing (`node-os-path-characterization.log`); after routing readdir/realpath/tmpdir to shared decode_node_path, `node-os-path-green.log` stays GREEN. This is not a claimed RED behavior fix. Tool grep over the entire crate finds zero to_string_lossy occurrences.

Native cause records: `native_error_causes_retain_node_error_metadata_and_utf16` directly compares5frozen failures (filesystem/unmatched outer path, NUL validation, deferred spawn, NUL spawn argument, callback/raw message). RED `node-error-causes-red.log` exposed std IO strings and omitted causes; GREEN `node-error-causes-green2.log` preserves native name/message/code/errno/syscall/path. NativeNodeError retains JsString human fields and JsValue properties; NodeRejection reuses that data authority. Numeric errno is a JS number in the native record. Stack text is omitted under explicit approval; none is fabricated. Default/custom in-flight abort nested causes are proven below.

## Abort cause proof and timing boundary

`in_flight_abort_errors_preserve_nested_native_cause_records` compares real FIFO readTextFile/readBinaryFile/writeFile/readTextLines failures with the default AbortController.abort() reason. REDs `node-abort-causes-red.log` and `node-abort-lines-causes-red2.log` omitted nested causes; GREEN `node-default-causes-green.log` contains19env groups+3readline groups. The error records retain AbortError/ABORT_ERR and nested default DOMException AbortError/code20, all native values. The old readLines CallbackError shim is deleted; one native abort-error constructor owns the data.

| Case | Status |
|---|---|
| Default controller.abort(), no custom reason | Proven above; no stack field fabricated |
| Supplied custom reason | `abort_reasons` compares actual frozen/native FIFO read causes for null/false/raw unmatched text/native Error. Shared AbortReason.value retains arbitrary JSON and default/timeout DOMException records; [carrier proof](shared-abort.md). No supplied reason is defaulted. |
| Generated JS stack strings | Approved omission (AGENTS.md2026-10-22); other native error data retained. |
| Post-start kernel FIFO read cancellation settlement before producer release | Not claimed by the cause-only fixture. The frozen ReadStream required producer release to settle. A first fixture accidentally retained its writer and hung; its owned cargo/oracle groups were terminated and OS child exit verified (`node-abort-hang-{processes,cleanup}.log`). The corrected fixture closes its producer at abort and awaits cleanup; this is not evidence that the pre-release timing window matches. |

## Unguarded legacy-shell transport

[Contract](../../specs/node-shell-transport.md). Frozen176–184 matches a normalized drive-letter System32/Sysnative bash.exe path regardless of process.platform, then310–325 sends the command on stdin with -s. `repin_node_legacy_wsl` proves16 actual frozen/native spawn cases, including Unicode Kelvin-sign drive lowercase, raw/NUL data,256KiB writes, early reader closure, inherited stdin after child exit and nonmatching argv control. Native input writes are polled with process/output handling; pipe errors are explicitly ignored like source. Pending stdin is dropped on child exit; inherited stdout/stderr still wait for EOF.

RED `node-legacy-wsl-red.log` found -c/argv instead of -s/stdin. Expanded RED `node-legacy-wsl-stdio-red.log` caught an incorrect wait for inherited stdin after child exit (native timeout versus source success); fixed by closing pending input on exit. Final GREEN `node-legacy-wsl-final-green.log`:19existing env groups plus the16case subprocess differential and child helper. No fixture asserts source substrings or shell-config construction.

| Windows-platform branch | Classification |
|---|---|
| findBashOnPath win32 where/bash.exe | gap: approved Windows-only discovery |
| getShellConfig win32 ProgramFiles/Git Bash candidates/no-shell result | gap: approved Windows-only discovery |
| killProcessTree win32 taskkill | gap: approved Windows-only process kill |
| node:path win32 absolute/resolve/join behavior and platform process options | gap: approved Windows-only path/platform behavior |

## Final-suite stale home-expansion assertion

Frozen `harness/env/nodejs.ts:47–49,257–258` resolves relative paths with node:path.resolve, without tilde expansion. `stale-expectations-oracle.ts` records /work/~/session.jsonl under both /overlay/home and /different/home; proof `stale-expectations-frozen-proof.log`. The old process_env_reads test expected an expansion absent from this pin; it now asserts the literal frozen path and retains the runtime HOME overlay. No production change.
