# Native detached cleanup

Frozen module: `packages/coding-agent/src/core/detached-job-cleanup.ts` at `4f9e6f8a4`.

Cleanup DB/session/artifact paths, process-reference sets, quarantine callbacks, selected/deleted paths and diagnostics retain `JsString`. Filesystem and stderr boundaries use Node UTF-8; realpath/readlink output is decoded back after the OS boundary. Date parsing reads native UTF-16. `run_detached_job_artifact_cleanup` takes `(&JsString, Option<&JsString>, Option<i64>)`.

Existing real DB/process-reference cleanup tests pass 8/8 through Cargo at `e0d77ff7` (`detached-cleanup-incrate-red.log`, despite its historical filename). No mock/copied control-DB harness.

The optional default root is `getAgentDir()`, a JS string: the configuration's OS `PathBuf` is decoded once as UTF-8 with replacement, as Node decodes env/home paths (same rule as `sdk.rs`/`settings_manager.rs`). Detached Bash's injected timestamp callback now returns a native string, matching terminal input; generated OS timestamps convert at their producer. Runner ownership compares native IDs directly. These API corrections remain unexecuted while the coding library is blocked.

## Source drift (authorized corrections pending)

1. Frozen source accepts artifactRoot and dirname(sessionPath), in that order, and canonicalizes the selected owning root. Real Cargo/frozen differential RED (`cleanup-roots-incrate-red-2.log`, fixture `9338c949`) deleted two source directories/16 bytes but only one native directory/8 bytes. Native now selects and canonicalizes that same first matching root; GREEN pending.
2. Rust wrapper currently performs expired-first selection and only checks all records over a global size cap. Frozen source directly invokes cleanup once.

Parent authorized correcting these separately after native migration, each with frozen behavioral RED first. This checkpoint does not claim frozen parity for either branch.
