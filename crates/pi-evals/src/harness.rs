//! The subset of the `vitest-evals` harness contract used by the Pi evals.
//!
//! `vitest-evals` is a third-party package; the port models only the values the
//! evals package produces and consumes: a normalized [`HarnessRun`], transcript
//! events, usage/timing summaries, per-run artifacts, and the [`Harness`]
//! adapter. OpenTelemetry trace attachment and `createHarness` failed-run
//! attachment belong to the third-party runner and are recorded as gaps in
//! `docs/porting/traceability/evals-*.md`.

use std::fmt;
use std::sync::Arc;

use pi_ai::request::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::error::PiEvalError;

pub type JsonObject = Map<String, Value>;

/// JSON-safe usage values (`UsageSummary`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub input_tokens: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub output_tokens: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub reasoning_tokens: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub total_tokens: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub tool_calls: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub retries: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonObject>,
}

/// JSON-safe timing values (`TimingSummary`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingSummary {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub total_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonObject>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
}

/// `NormalizedError` as produced for failed tool results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedError {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

impl NormalizedError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            r#type: None,
            extra: JsonObject::new(),
        }
    }
}

/// Ordered transcript event captured in a harness session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptEvent {
    Message {
        role: TranscriptRole,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<JsonObject>,
    },
    ToolCall {
        id: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arguments: Option<JsonObject>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        duration_ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<JsonObject>,
    },
    ToolResult {
        tool_call_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<NormalizedError>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        duration_ms: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metadata: Option<JsonObject>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedSession {
    pub events: Vec<TranscriptEvent>,
}

/// Normalized run returned by a [`Harness`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessRun {
    pub output: Value,
    pub session: NormalizedSession,
    pub usage: UsageSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<TimingSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<JsonObject>,
    pub errors: Vec<JsonObject>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallStatus {
    Pending,
    Error,
    Ok,
}

/// Tool call paired with its result, as consumed by judges (`toolCalls(run)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<JsonObject>,
    pub status: ToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<NormalizedError>,
}

/// `toolCalls(run)`: pairs each call with the first result carrying its id.
pub fn tool_calls(run: &HarnessRun) -> Vec<ToolCall> {
    let mut results_by_id: indexmap::IndexMap<&str, &TranscriptEvent> = indexmap::IndexMap::new();
    for event in &run.session.events {
        if let TranscriptEvent::ToolResult { tool_call_id, .. } = event {
            results_by_id.entry(tool_call_id).or_insert(event);
        }
    }
    run.session
        .events
        .iter()
        .filter_map(|event| match event {
            TranscriptEvent::ToolCall {
                id, name, arguments, ..
            } => Some((id, name, arguments)),
            _ => None,
        })
        .map(|(id, name, arguments)| {
            let call = ToolCall {
                name: name.clone(),
                arguments: arguments.clone(),
                status: ToolCallStatus::Pending,
                result: None,
                error: None,
            };
            match results_by_id.get(id.as_str()) {
                None => call,
                Some(TranscriptEvent::ToolResult { error, content, .. }) => match error {
                    Some(error) => ToolCall {
                        status: ToolCallStatus::Error,
                        error: Some(error.clone()),
                        ..call
                    },
                    None => ToolCall {
                        status: ToolCallStatus::Ok,
                        result: content.clone(),
                        ..call
                    },
                },
                Some(_) => call,
            }
        })
        .collect()
}

/// Per-run context shared with the harness (`HarnessContext`).
#[derive(Debug, Clone, Default)]
pub struct HarnessContext {
    pub artifacts: JsonObject,
    pub signal: Option<CancellationToken>,
}

impl HarnessContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// `context.setArtifact(name, value)`.
    pub fn set_artifact(&mut self, name: impl Into<String>, value: Value) {
        self.artifacts.insert(name.into(), value);
    }
}

pub type HarnessRunFuture = BoxFuture<'static, Result<HarnessRun, PiEvalError>>;

/// Adapter that executes the system under test and returns a normalized run.
pub struct Harness<I> {
    pub name: String,
    run: Arc<dyn Fn(I, HarnessContext) -> HarnessRunFuture + Send + Sync>,
}

impl<I> Clone for Harness<I> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            run: Arc::clone(&self.run),
        }
    }
}

impl<I: Send + 'static> Harness<I> {
    pub fn new(
        name: impl Into<String>,
        run: impl Fn(I, HarnessContext) -> HarnessRunFuture + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            run: Arc::new(run),
        }
    }

    pub fn run(&self, input: I, context: HarnessContext) -> HarnessRunFuture {
        (self.run)(input, context)
    }
}

impl<I> fmt::Debug for Harness<I> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Harness")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
