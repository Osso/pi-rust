# Native detached cleanup

Frozen module: `packages/coding-agent/src/core/detached-job-cleanup.ts` at `4f9e6f8a4`.

Cleanup DB/session/artifact paths, process-reference sets, quarantine callbacks, selected/deleted paths and diagnostics retain `JsString`. Filesystem and stderr boundaries use Node UTF-8; realpath/readlink output is decoded back after the OS boundary. Date parsing reads native UTF-16. `run_detached_job_artifact_cleanup` takes `(&JsString, Option<&JsString>, Option<i64>)`.

Proof: existing real DB and process-reference tests in `tests/detached_job_cleanup.rs` adapted to native APIs; execution pending coding-library compilation. No mock/copied control-DB harness.

## Source drift (authorized corrections pending)

1. Rust currently accepts artifactRoot alone; frozen source also accepts dirname(sessionPath), needed for Pyrun artifacts.
2. Rust wrapper currently performs expired-first selection and only checks all records over a global size cap. Frozen source directly invokes cleanup once.

Parent authorized correcting these separately after native migration, each with frozen behavioral RED first. This checkpoint does not claim frozen parity for either branch.
