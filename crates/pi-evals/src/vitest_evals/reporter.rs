//! Port of `packages/evals/src/vitest-evals/reporter.ts`.
//!
//! The TypeScript module is a Vitest reporter. The Rust port keeps the same
//! two hooks as plain methods over the ported test-run model: per-test-case
//! `runs.jsonl` appends and the end-of-run comparison report.

use pi_ai::utils::uuid::random_uuid;
use pi_coding_agent::utils::js_json;
use pi_js_compat::process_env;
use pi_js_compat::string::js_trim;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::io::AsyncWriteExt;

use crate::error::PiEvalError;
use crate::harness::{HarnessRun, UsageSummary};
use crate::vitest_evals::artifacts::{PI_SESSION_SNAPSHOT_ARTIFACT, TestArtifact, persist_eval_artifact_references};
use crate::vitest_evals::harness_table::{EVAL_HARNESS_ITERATION_ARTIFACT, parse_eval_harness_iteration_artifact};
use crate::vitest_evals::summary::{
    HarnessObservation, HarnessObservationBase, HarnessObservationOutcome, format_harness_comparison_report,
    summarize_harness_comparisons,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestState {
    Passed,
    Failed,
    Skipped,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunEndReason {
    Passed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct HarnessMeta {
    pub name: String,
    pub run: HarnessRun,
}

#[derive(Debug, Clone, Default)]
pub struct EvalMeta {
    pub avg_score: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct EvalTestCase {
    pub id: String,
    pub file: String,
    pub name: String,
    pub full_name: String,
    pub state: TestState,
    pub artifacts: Vec<TestArtifact>,
    pub harness: Option<HarnessMeta>,
    pub eval: Option<EvalMeta>,
}

#[derive(Debug, Clone)]
pub struct TestModule {
    pub relative_module_id: String,
    pub tests: Vec<EvalTestCase>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRecordTest<'a> {
    id: &'a str,
    file: &'a str,
    name: &'a str,
    full_name: &'a str,
    status: TestState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRecord<'a> {
    schema_version: u64,
    run_id: &'a str,
    test: RunRecordTest<'a>,
    harness: &'a str,
    usage: &'a UsageSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    timings: Option<&'a crate::harness::TimingSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<&'a [Map<String, Value>]>,
    artifacts: Vec<crate::vitest_evals::artifacts::ArtifactReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<Map<String, Value>>,
}

fn artifact_directory() -> Option<String> {
    process_env::get("PI_EVAL_ARTIFACT_DIR")
        .map(|directory| js_trim(&directory).to_owned())
        .filter(|directory| !directory.is_empty())
}

fn read_finite_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|value| value.is_finite())
}

/// `appendHarnessRunReport(test)`.
pub async fn append_harness_run_report(test: &EvalTestCase) -> Result<(), PiEvalError> {
    let Some(artifact_directory) = artifact_directory() else {
        return Ok(());
    };
    let Some(harness) = test.harness.as_ref() else {
        return Ok(());
    };
    let run = &harness.run;
    let run_id = match run.artifacts.as_ref().and_then(|artifacts| artifacts.get("runId")) {
        Some(Value::String(run_id)) => run_id.clone(),
        _ => random_uuid(),
    };
    let metadata: Map<String, Value> = run
        .artifacts
        .iter()
        .flat_map(|artifacts| artifacts.iter())
        .filter(|(name, _)| name.as_str() != "runId" && name.as_str() != PI_SESSION_SNAPSHOT_ARTIFACT)
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let record = RunRecord {
        schema_version: 1,
        run_id: &run_id,
        test: RunRecordTest {
            id: &test.id,
            file: &test.file,
            name: &test.name,
            full_name: &test.full_name,
            status: test.state,
        },
        harness: &harness.name,
        usage: &run.usage,
        timings: run.timings.as_ref(),
        errors: (!run.errors.is_empty()).then_some(run.errors.as_slice()),
        artifacts: persist_eval_artifact_references(&test.artifacts, &run_id, &artifact_directory)?,
        metadata: (!metadata.is_empty()).then_some(metadata),
    };
    let record = serde_json::to_value(&record).map_err(|error| PiEvalError::error(error.to_string()))?;
    tokio::fs::create_dir_all(&artifact_directory)
        .await
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    let path = std::path::Path::new(&artifact_directory).join("runs.jsonl");
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    let line = format!("{}\n", js_json::stringify(&record));
    file.write_all(line.as_bytes())
        .await
        .map_err(|error| PiEvalError::error(error.to_string()))?;
    file.flush()
        .await
        .map_err(|error| PiEvalError::error(error.to_string()))
}

/// `collectHarnessObservations(modules)`.
pub fn collect_harness_observations(modules: &[TestModule]) -> Vec<HarnessObservation> {
    let mut observations = Vec::new();
    for module in modules {
        for test in &module.tests {
            let Some(harness) = test.harness.as_ref() else {
                continue;
            };
            let run = &harness.run;
            let iteration = parse_eval_harness_iteration_artifact(
                run.artifacts
                    .as_ref()
                    .and_then(|artifacts| artifacts.get(EVAL_HARNESS_ITERATION_ARTIFACT)),
            );
            let Some(iteration) = iteration else {
                continue;
            };
            let score = test
                .eval
                .as_ref()
                .and_then(|eval| eval.avg_score)
                .filter(|score| score.is_finite());
            let estimated_cost_usd = read_finite_number(
                run.usage
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("estimatedCostUsd")),
            );
            let base = HarnessObservationBase {
                eval_set: iteration.eval_set.clone(),
                group_key: iteration.group_key.clone(),
                test_name: test.name.clone(),
                file: module.relative_module_id.clone(),
                harness: iteration.harness.clone(),
                baseline: iteration.baseline.clone(),
                candidates: iteration.candidates.clone(),
                repetition: iteration.repetition,
                total_tokens: run.usage.total_tokens.filter(|value| value.is_finite()),
                total_ms: run
                    .timings
                    .as_ref()
                    .and_then(|timings| timings.total_ms)
                    .filter(|value| value.is_finite()),
                estimated_cost_usd,
            };
            let (outcome, score) = if !run.errors.is_empty() {
                (HarnessObservationOutcome::Errored, None)
            } else if let Some(score) = score {
                (HarnessObservationOutcome::Scored, Some(score))
            } else {
                let outcome = match test.state {
                    TestState::Passed => HarnessObservationOutcome::Unscored,
                    TestState::Failed => HarnessObservationOutcome::Errored,
                    TestState::Skipped => HarnessObservationOutcome::Skipped,
                    TestState::Pending => HarnessObservationOutcome::Pending,
                };
                (outcome, None)
            };
            observations.push(HarnessObservation { base, outcome, score });
        }
    }
    observations
}

/// `EvalHarnessReporter`.
#[derive(Debug, Clone, Default)]
pub struct EvalHarnessReporter;

impl EvalHarnessReporter {
    pub async fn on_test_case_result(&self, test: &EvalTestCase) -> Result<(), PiEvalError> {
        append_harness_run_report(test).await
    }

    /// Returns the text the TypeScript reporter logs through `vitest.logger`.
    pub fn on_test_run_end(
        &self,
        modules: &[TestModule],
        reason: TestRunEndReason,
    ) -> Result<Option<String>, PiEvalError> {
        if reason == TestRunEndReason::Interrupted {
            return Ok(Some("\nEval comparisons unavailable: test run interrupted.".to_owned()));
        }
        let report = summarize_harness_comparisons(&collect_harness_observations(modules));
        let formatted = format_harness_comparison_report(&report);
        Ok((!formatted.is_empty()).then(|| format!("\n{formatted}")))
    }
}
