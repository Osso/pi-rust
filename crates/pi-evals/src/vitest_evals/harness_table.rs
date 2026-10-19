//! Port of `packages/evals/src/vitest-evals/harness-table.ts`.
//!
//! Plans baseline/candidate repetitions for comparative eval sets and derives
//! the group key used to pair observations across harnesses.

use std::sync::Arc;

use pi_js_compat::js_json;
use pi_js_compat::string::js_trim;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::PiEvalError;
use crate::harness::{Harness, HarnessRunFuture};

pub const EVAL_HARNESS_ITERATION_ARTIFACT: &str = "vitestEvalsHarnessIteration";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalHarnessIterationArtifact {
    pub schema_version: u64,
    pub eval_set: String,
    pub harness: String,
    pub baseline: String,
    pub candidates: Vec<String>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub repetition: f64,
    pub group_key: String,
}

/// Parsed artifact in the source's `parseEvalHarnessIterationArtifact` literal
/// order (the created artifact spreads `plan` first and appends `groupKey`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedEvalHarnessIterationArtifact {
    pub schema_version: u64,
    pub eval_set: String,
    pub group_key: String,
    pub harness: String,
    pub baseline: String,
    pub candidates: Vec<String>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub repetition: f64,
}

#[derive(Clone)]
pub struct EvalHarnessTableRow<I> {
    pub harness: Harness<I>,
    pub name: String,
    pub repetition: u32,
}

pub struct EvalHarnessTablePairOptions<I> {
    pub baseline: Harness<I>,
    pub candidate: Harness<I>,
    pub repetitions: Option<u32>,
}

pub struct EvalHarnessTableCandidatesOptions<I> {
    pub baseline: Harness<I>,
    pub candidates: Vec<Harness<I>>,
    pub repetitions: Option<u32>,
}

/// `EvalHarnessTableOptions` union.
pub enum EvalHarnessTableOptions<I> {
    Pair(EvalHarnessTablePairOptions<I>),
    Candidates(EvalHarnessTableCandidatesOptions<I>),
}

impl<I> EvalHarnessTableOptions<I> {
    fn repetitions(&self) -> Option<u32> {
        match self {
            Self::Pair(options) => options.repetitions,
            Self::Candidates(options) => options.repetitions,
        }
    }

    fn baseline(&self) -> &Harness<I> {
        match self {
            Self::Pair(options) => &options.baseline,
            Self::Candidates(options) => &options.baseline,
        }
    }

    fn candidates(&self) -> Vec<&Harness<I>> {
        match self {
            Self::Pair(options) => vec![&options.candidate],
            Self::Candidates(options) => options.candidates.iter().collect(),
        }
    }
}

/// `parseEvalHarnessIterationArtifact`.
pub fn parse_eval_harness_iteration_artifact(value: Option<&Value>) -> Option<ParsedEvalHarnessIterationArtifact> {
    let object = value?.as_object()?;
    if object.get("schemaVersion")?.as_f64()? != 1.0 {
        return None;
    }
    let candidates = object.get("candidates")?.as_array()?;
    let candidates = candidates.iter().map(Value::as_str).collect::<Option<Vec<&str>>>()?;
    Some(ParsedEvalHarnessIterationArtifact {
        schema_version: 1,
        eval_set: object.get("evalSet")?.as_str()?.to_owned(),
        group_key: object.get("groupKey")?.as_str()?.to_owned(),
        harness: object.get("harness")?.as_str()?.to_owned(),
        baseline: object.get("baseline")?.as_str()?.to_owned(),
        candidates: candidates.into_iter().map(str::to_owned).collect(),
        repetition: object.get("repetition")?.as_f64()?,
    })
}

/// `canonicalizeJson`: recursively sorts object keys by UTF-16 code unit and
/// keeps array order.
fn canonicalize_json(value: &Value) -> Result<Value, PiEvalError> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(value.clone()),
        Value::Number(number) => match number.as_f64() {
            Some(number) if number.is_finite() => Ok(value.clone()),
            _ => Err(PiEvalError::type_error("Eval input must contain only finite numbers.")),
        },
        Value::Array(items) => items
            .iter()
            .map(canonicalize_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(object) => {
            let mut entries: Vec<(&String, &Value)> = object.iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
            let mut canonical = Map::new();
            for (key, item) in entries {
                canonical.insert(key.clone(), canonicalize_json(item)?);
            }
            Ok(Value::Object(canonical))
        }
    }
}

fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn derive_input_key(input: &Value) -> Result<String, PiEvalError> {
    if let Value::Object(object) = input
        && let Some(Value::String(id)) = object.get("id")
    {
        let id = js_trim(id);
        if !id.is_empty() {
            return Ok(id.to_owned());
        }
    }
    let canonical = canonicalize_json(input)?;
    Ok(sha256_hex(&js_json::stringify(&canonical)))
}

/// `deriveEvalGroupKey(input, repetition)`.
pub fn derive_eval_group_key(input: &Value, repetition: f64) -> Result<String, PiEvalError> {
    let input_key = derive_input_key(input)?;
    Ok(js_json::stringify(&Value::Array(vec![
        Value::String(input_key),
        pi_js_compat::number::js_number_value(repetition),
    ])))
}

fn validate_options<I>(
    eval_set: &str,
    baseline: &Harness<I>,
    candidates: &[&Harness<I>],
    repetitions: u32,
) -> Result<(), PiEvalError> {
    if js_trim(eval_set).is_empty() {
        return Err(PiEvalError::type_error("evalSet must not be empty."));
    }
    if candidates.is_empty() {
        return Err(PiEvalError::type_error("At least one candidate harness is required."));
    }
    let mut names = std::collections::HashSet::new();
    names.insert(baseline.name.as_str());
    for candidate in candidates {
        names.insert(candidate.name.as_str());
    }
    if names.len() != candidates.len() + 1 {
        return Err(PiEvalError::type_error(
            "Harness names must be unique within an eval set.",
        ));
    }
    if repetitions < 1 {
        return Err(PiEvalError::type_error("repetitions must be a positive integer."));
    }
    Ok(())
}

struct IterationPlan {
    schema_version: u64,
    eval_set: String,
    harness: String,
    baseline: String,
    candidates: Vec<String>,
    repetition: u32,
}

fn with_iteration_artifact<I: Serialize + Send + 'static>(
    harness: &Harness<I>,
    plan: Arc<IterationPlan>,
) -> Harness<I> {
    let name = harness.name.clone();
    let inner = harness.clone();
    Harness::new(name, move |input, mut context| -> HarnessRunFuture {
        let inner = inner.clone();
        let plan = Arc::clone(&plan);
        Box::pin(async move {
            let input_value = serde_json::to_value(&input)
                .map_err(|_| PiEvalError::type_error("Eval input must be JSON-serializable."))?;
            let group_key = derive_eval_group_key(&input_value, f64::from(plan.repetition))?;
            let artifact = EvalHarnessIterationArtifact {
                schema_version: plan.schema_version,
                eval_set: plan.eval_set.clone(),
                harness: plan.harness.clone(),
                baseline: plan.baseline.clone(),
                candidates: plan.candidates.clone(),
                repetition: f64::from(plan.repetition),
                group_key,
            };
            let artifact_value =
                serde_json::to_value(&artifact).expect("eval harness iteration artifacts must serialize");
            context.set_artifact(EVAL_HARNESS_ITERATION_ARTIFACT, artifact_value.clone());
            let mut run = inner.run(input, context.clone()).await?;
            let mut artifacts = context.artifacts;
            if let Some(existing) = run.artifacts.take() {
                for (key, value) in existing {
                    artifacts.insert(key, value);
                }
            }
            artifacts.insert(EVAL_HARNESS_ITERATION_ARTIFACT.to_owned(), artifact_value);
            run.artifacts = Some(artifacts);
            Ok(run)
        })
    })
}

/// `evalHarnessTable(evalSet, options)`.
pub fn eval_harness_table<I: Serialize + Send + 'static>(
    eval_set: &str,
    options: EvalHarnessTableOptions<I>,
) -> Result<Vec<EvalHarnessTableRow<I>>, PiEvalError> {
    let repetitions = options.repetitions().unwrap_or(1);
    let baseline = options.baseline().clone();
    let candidates = options.candidates();
    validate_options(eval_set, &baseline, &candidates, repetitions)?;

    let candidate_names: Vec<String> = candidates.iter().map(|harness| harness.name.clone()).collect();
    let mut rows = Vec::new();
    for repetition in 1..=repetitions {
        for harness in std::iter::once(&baseline).chain(candidates.iter().copied()) {
            let plan = Arc::new(IterationPlan {
                schema_version: 1,
                eval_set: eval_set.to_owned(),
                harness: harness.name.clone(),
                baseline: baseline.name.clone(),
                candidates: candidate_names.clone(),
                repetition,
            });
            rows.push(EvalHarnessTableRow {
                harness: with_iteration_artifact(harness, plan),
                name: harness.name.clone(),
                repetition,
            });
        }
    }
    Ok(rows)
}
