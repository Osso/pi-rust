# Detached job cleanup

Whole module audit: `packages/coding-agent/src/core/detached-job-cleanup.ts`, common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-storage/src/core/detached_job_cleanup.rs`.

| Frozen symbol / branch | Native mapping / proof |
| --- | --- |
| `cleanupDetachedJobArtifacts`, `emptyCleanupResult` | `cleanup_detached_job_artifacts`: Linux inspection, all-terminal candidates, three-day/two-GiB selection, deletion totals. Ten real DB/filesystem tests GREEN at adde11a9e. |
| `runDetachedJobArtifactCleanup` | Existing wrapper uses the same whole selection path and stderr prefixes; frozen wrapper over-cap differential. |
| `collectTerminalArtifactCandidates` | Insertion-ordered path deduplication; duplicate paths become protected. |
| `terminalArtifactCandidate`, `detachedOutputPath` | Terminal-only, first matching Bash/Pyrun output reference, finite date, measured bytes, live references. Invalid timestamp/active/recent tests. |
| `detachedArtifactDirectory`, `isPathSegment` | Absolute output.log, valid job/session segments, agent-root OR session-root, non-symlink directory, canonical-root containment. Frozen two-root differential plus traversal/outside-root tests. |
| `readDirectoryByteSize`, `totalCandidateBytes` | Iterative directory traversal; lstat leaf size; sparse-file byte-cap selection. |
| `deleteSelectedArtifactDirectories`, `deleteUnreferencedArtifactDirectory` | Source candidate order and byte totals; UUID quarantine, callback, reference recheck, restore or removal; real quarantine/open-fd tests. |
| `quarantinedDirectoryHasLiveReference`, `restoreQuarantinedDirectory` | Protect a directory while referenced; restores original name before returning without deletion. |
| Export `readLinuxProcessReferences` | Now public, matching frozen export (was pub(crate)). Missing public-export RED `/tmp/runtime-services-red-cleanup-export.log`; current frozen/native live/deleted descriptor proof pending at commit. |
| `addLinkReference`, `addDescriptorReferences`, `addAbsoluteReference` | cwd/exe/fd enumeration; strip ` (deleted)`, absolute paths only, canonicalize or lexical resolve for expected resolution errors; real descriptor differential. |
| `directoryHasLiveReference` | Exact directory OR slash-delimited descendant, not string-prefix lookalikes. |
| `isTerminalLifecycle`, `isMissingPathError`, expected-error predicates, `isErrorCode`, `errorMessage` | Native enum/errno matching and human-error text boundaries. |

Current commands: `cargo test -j3 -p pi-coding-agent --test suite_core_1 detached_job_cleanup` (10 pass at adde11a9e, `/tmp/runtime-services-cleanup.log`); new export/descriptor test pending. Original expired-only helper/byte-count APIs do not exist in the new source or Rust; removed stale historical trace rows.

Gap: non-Linux /proc inspection and adversarial/OS-fault/race shapes, including failed quarantine restoration/inspection diagnostics, remain outside the user-observable realistic-input parity bar. Native numbers use i64 totals; numeric extremes excluded. Config/default-root errors propagate explicitly; native textual callback/returned-reference identity is excluded by the standing owned-value approval.
