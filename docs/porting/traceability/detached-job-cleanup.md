# coding-agent detached-job-cleanup traceability

Pinned source: `packages/coding-agent/src/core/detached-job-cleanup.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent-core/src/core/detached_job_cleanup.rs`. Tests: `crates/pi-coding-agent/tests/detached_job_cleanup.rs` (8 behavioral, real `/proc`, sparse files).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `cleanupDetachedJobArtifacts` (Linux `/proc` gate, selection, deletion) | `cleanup_detached_job_artifacts` returning `Result` (TS throws on selection errors) | all tests |
| `runDetachedJobArtifactCleanup` (expired sweep, over-cap sweep, `console.error`) | `run_detached_job_artifact_cleanup` (`eprintln!`) | over-cap test exercises the same selection path |
| `cleanupExpiredDetachedJobArtifacts` (cutoff `now - 3d`, no byte cap) | `cleanup_expired_detached_job_artifacts` | `deletes_expired_terminal_artifacts_and_preserves_nonterminal_directories` |
| `detachedArtifactBytes` | `detached_artifact_bytes` | over-cap test |
| `collectTerminalArtifactCandidates` (dedupe by path, mark protected) | `collect_terminal_artifact_candidates` | open-descriptor test |
| `terminalArtifactCandidate` (terminal lifecycle, output ref, invalid timestamp error, byte size) | `terminal_artifact_candidate` | `reports_invalid_terminal_timestamps_without_deleting` |
| `detachedOutputPath` (`Bash output`/`Pyrun output`) | `detached_output_path` | all artifact tests |
| `detachedArtifactDirectory` (absolute `output.log`, path segments, resolved/real paths) | `detached_artifact_directory`, `resolve_path`, `is_path_segment`, `session_stem` | `refuses_an_artifact_path_outside_the_agent_artifact_root`, `refuses_a_detached_job_id_containing_path_traversal` |
| `readDirectoryByteSize` | `read_directory_byte_size` | over-cap test |
| `readLinuxProcessReferences` + `cwd`/`exe`/`fd` links, `(deleted)` suffix | `read_linux_process_references`, `add_link_reference`, `add_descriptor_references`, `add_absolute_reference` | `preserves_a_terminal_directory_containing_an_open_file_descriptor` |
| shared idle-archive dependency (2026-10-23) | scan exposed pub(crate) unchanged under parent instruction | Existing unexpected-IO errors are flattened to JsString; native source/errno/console carrier parity is deferred, not repaired or newly accepted by the archive slice |
| `directoryHasLiveReference` | `directory_has_live_reference` | open-descriptor test |
| `deleteUnreferencedArtifactDirectory` (quarantine, live-reference restore, rm) | `delete_unreferenced_artifact_directory`, `quarantined_directory_has_live_reference`, `restore_quarantined_directory` | `reports_quarantine_paths_and_removes_them_on_success` |
| `onDirectoryQuarantined` | `DetachedJobCleanupOptions::on_directory_quarantined` | quarantine test |
| `emptyCleanupResult("live process reference inspection requires Linux /proc")` | `empty_cleanup_result` | non-Linux only (Rust test host is Linux) |
| Error codes `ENOENT`/`EACCES`/`ENOTDIR`/`EPERM`/`ESRCH`/`ENAMETOOLONG` | raw errno checks (2/13/20/1/3/36) | open-descriptor and missing-path paths |

Gaps / deviations:

- `readLinuxProcessReferences` returns `Result<Option<…>>`; unexpected `/proc` read errors propagate instead of being swallowed.
- `quarantinedDirectoryHasLiveReference` treats an unreadable `/proc` or vanished realpath as live (the source would throw); this preserves the safe outcome (no deletion).
- `runDetachedJobArtifactCleanup` writes `console.error` equivalents to stderr.
