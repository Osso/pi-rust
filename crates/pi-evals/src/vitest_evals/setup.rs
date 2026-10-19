//! Port of `packages/evals/src/vitest-evals/setup.ts`.
//!
//! The TypeScript `afterEach` hook registers the native session snapshot of a
//! harness run against the explicit test task. The Rust runner calls this after
//! each eval case.

use crate::error::PiEvalError;
use crate::vitest_evals::artifacts::record_eval_session_artifact;
use crate::vitest_evals::reporter::EvalTestCase;

/// `afterEach` body of the eval setup file.
pub fn record_eval_session_artifact_after_run(test: &mut EvalTestCase) -> Result<(), PiEvalError> {
    let Some(harness) = test.harness.as_ref() else {
        return Ok(());
    };
    record_eval_session_artifact(&mut test.artifacts, &harness.run)
}
