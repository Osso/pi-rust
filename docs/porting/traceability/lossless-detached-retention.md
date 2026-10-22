# Native detached retention

Frozen module: `packages/coding-agent/src/core/detached-job-retention.ts` at `4f9e6f8a4`.

All selected directory paths and validation diagnostics use `JsString`. Equal-timestamp path ordering calls the native ICU UTF-16 comparator, preserving raw units. Age/cap selection and live-reference protection are unchanged.

`tests/lossless_retention.rs` asserts raw selected paths and raw error interpolation. Actual-module standalone type RED: `/tmp/pi-rust-repin-lossless-tools/retention-red.log` (old scalar contract cannot accept the raw fixture). This is type evidence, not a behavioral RED. Runtime GREEN and behavioral mutants remain pending at this checkpoint. Existing retention behavior tests remain in `tests/detached_job_retention.rs`.
