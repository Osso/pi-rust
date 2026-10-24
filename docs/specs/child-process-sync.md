# Synchronous child-process capture

One `utils::child_process::spawn_sync` authority replaces the old `spawn_process_sync` runner; no compatibility adapter remains. Frozen clipboard-image.ts:86–114 supplies explicit timeout and maxBuffer values.

- Typed arguments stay UTF-16 through Node argument validation, then encode at the OS boundary. Validation throws retain native Error identity/properties.
- Explicit stdio supports pipe/ignore/null entries: null uses Node's pipe default, ignored output returns null rather than captured bytes. Input bytes are delivered only to piped stdin, then stdin closes. Existing callers explicitly retain ignored stdin plus piped output defaults.
- The timeout covers input delivery and output/exit waiting together; a blocked stdin writer must not postpone the deadline. Async spawn uses the same explicit stdio choices, without adding a second spawn path.
- Spawn failures return pid0, null pipes/status/signal and an errno-bearing Error with code, syscall, path and spawnargs. Runtime failures retain captured pipes, pid, exit status and signal.
- Configured timeout sends killSignal (default SIGTERM), then waits for termination and pipe EOF. It never escalates to SIGKILL. Timeout remains ETIMEDOUT even when the child handles SIGTERM and exits0.
- Configured maxBuffer0 disables the cap, matching frozen Node. Positive maxBuffer monitors the combined bytes of both pipes and reports ENOBUFS; both streams are drained without deadlocking. No output/termination failure is silently discarded.
- Existing shell/project-resolver callers retain their unbounded native capture policy when no limits are supplied. Reproducing an implicit Node default maxBuffer remains excluded by the existing runtime-oddity decision; clipboard's explicit limits are not excluded.

Owned proof: clipboard actual-source/fake-process differential plus direct Node spawnSync result/error probes. Existing sync cwd/status and CLI environment tests migrate to this authority; shell lookup and remote-name consumers require focused regressions. Windows execution remains the project-wide gap.
