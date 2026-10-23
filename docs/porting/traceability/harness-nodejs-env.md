# Node execution environment — common-pin traceability

Frozen: `packages/agent-core/src/harness/env/nodejs.ts`, immutable `4f9e6f8a4`. Native: `crates/pi-agent-core/src/harness/env/nodejs.rs` and `nodejs/read_lines.rs`. Status **partial**: Windows classification and final caller-adapter proof remain open. No old-pin proof upgrades this row.

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

- Rejection carrier behavior is implemented/proven below; pre-existing API fixtures still need adaptation before the checkpoint can be integrated.
- Windows-only discovery/taskkill/legacy WSL stdin transport: user classification pending. Not ported or claimed. The WSL-path helper is unguarded even on POSIX; no Windows-path parity claim is made.
- Raw OS path ingress (cwd, readdir names, realpath and tmpdir) uses the shared node_fs decoder. A full crates/pi-agent-core grep finds no remaining to_string_lossy sites.

Removed old-only renameFile, inheritEnv, path expansion, custom missing-cwd guard, child tracking and100ms stdio grace. No compatibility fallback retained.

## Native IO rejection carrier

Parent authorized2026-10-22. `OperationFailure<E>::Returned(E)` means the frozen promise resolved to err(E); `Rejected(NodeRejection)` means it rejected with the original native Node error. NodeRejection retains name/code/errno/syscall/message. Only FS/Shell methods whose frozen resolvePath is outside try gain that carrier; join/temp methods keep their existing returned-result APIs. The cwd expect panic is removed.

`node-rejection-red.log` reproduces the real deleted-cwd panic in an isolated process. `node-rejection-green.log` compares raw Node uv_cwd ENOENT properties. `node-rejection-callers-green2.log` compares18source/native outcomes:12resolving FS methods, exec with requested cwd, join (still succeeds), prompt/skill loaders, JSONL repo and shell capture. Prompt/skill/JSONL callers preserve rejection rather than generate diagnostics or a prefixed SessionError. JSONL repository operations and session writes expose SessionFailure; pure session getters and memory-repo APIs are unchanged. Frozen shell capture has its own try/catch: it returns unknown ExecutionError with the original native Node cause, not a rejection. The test asserts that distinction explicitly.

Legacy fixture adaptation preserves all existing assertions and now explicitly fails on an unexpected rejected promise before inspecting a returned backend error. `node-rejection-adapters-development4.log`:50 focused caller tests GREEN across11targets, including frozen resource/session/repository/capture matrices and private POSIX discovery. This is bounded development proof, not the final full package suite.

OS path authority follow-up: `raw_os_filename_and_realpath_bytes_match_node_repair_boundary` uses a real filename containing FF and ED A0 80 bytes plus an ASCII symlink. Frozen realpath returns a UTF-8-repaired path; frozen listDir reconstructs that repaired name and fails lstat although the original file exists (DISCREPANCY retained). Characterization was already GREEN before the mechanical helper routing (`node-os-path-characterization.log`); after routing readdir/realpath/tmpdir to shared decode_node_path, `node-os-path-green.log` stays GREEN. This is not a claimed RED behavior fix. Tool grep over the entire crate finds zero to_string_lossy occurrences.
