//! Port of `packages/agent/src/harness/telemetry.ts`.
//!
//! The TypeScript module declares two telemetry schemas and two typed span
//! starters. Rust cannot construct the schema maps in constants, so the
//! exported values are backed by `LazyLock` statics and returned by reference.

use std::collections::BTreeMap;
use std::error::Error;
use std::future::Future;
use std::sync::LazyLock;

use pi_telemetry::{
    SpanAttributes, SpanOptions, TelemetryAttributeCardinality, TelemetryAttributeDefinition,
    TelemetryAttributeMetadata, TelemetryAttributeValueDefinition, TelemetryContext, TelemetryContextExt,
    TelemetryDefaultStatus, TelemetryParentDefinition, TelemetryRequiredAttributeDefinition, TelemetrySchemaDefinition,
    TelemetrySpan, TelemetrySpanDefinition, TelemetrySpanStatusDefinition,
};

/// `AiSpanName`: the AI request span declared by [`ai_telemetry_schema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSpanName {
    PiAiRequest,
}

impl AiSpanName {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PiAiRequest => "pi.ai.request",
        }
    }
}

/// `HarnessSpanName`: the harness spans declared by [`harness_telemetry_schema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessSpanName {
    PiHarnessRun,
    PiHarnessCompaction,
    PiHarnessNavigation,
    PiHarnessCheckpoint,
    PiHarnessTurn,
    PiHarnessStep,
    PiHarnessTool,
    PiHarnessHook,
    PiHarnessSleep,
    PiHarnessEventHandler,
    PiSessionWrite,
}

impl HarnessSpanName {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PiHarnessRun => "pi.harness.run",
            Self::PiHarnessCompaction => "pi.harness.compaction",
            Self::PiHarnessNavigation => "pi.harness.navigation",
            Self::PiHarnessCheckpoint => "pi.harness.checkpoint",
            Self::PiHarnessTurn => "pi.harness.turn",
            Self::PiHarnessStep => "pi.harness.step",
            Self::PiHarnessTool => "pi.harness.tool",
            Self::PiHarnessHook => "pi.harness.hook",
            Self::PiHarnessSleep => "pi.harness.sleep",
            Self::PiHarnessEventHandler => "pi.harness.event_handler",
            Self::PiSessionWrite => "pi.session.write",
        }
    }
}

/// `AiSpan`: runtime span admitted by the AI schema.
pub type AiSpan = TelemetrySpan;
/// `AiTelemetrySpan`: runtime span admitted by the AI schema.
pub type AiTelemetrySpan = TelemetrySpan;
/// `AiSpanAttributes`: runtime attributes of an AI span.
pub type AiSpanAttributes = SpanAttributes;
/// `AiSpanStartAttributes`: start attributes of an AI span.
pub type AiSpanStartAttributes = SpanAttributes;
/// `AiSpanEndAttributes`: end attributes of an AI span.
pub type AiSpanEndAttributes = SpanAttributes;
/// `HarnessSpan`: runtime span admitted by the harness schema.
pub type HarnessSpan = TelemetrySpan;
/// `HarnessTelemetrySpan`: runtime span admitted by the harness schema.
pub type HarnessTelemetrySpan = TelemetrySpan;
/// `HarnessSpanAttributes`: runtime attributes of a harness span.
pub type HarnessSpanAttributes = SpanAttributes;
/// `HarnessSpanStartAttributes`: start attributes of a harness span.
pub type HarnessSpanStartAttributes = SpanAttributes;
/// `HarnessSpanEndAttributes`: end attributes of a harness span.
pub type HarnessSpanEndAttributes = SpanAttributes;

/// `AI_TELEMETRY_SCHEMA`.
pub fn ai_telemetry_schema() -> &'static TelemetrySchemaDefinition {
    static SCHEMA: LazyLock<TelemetrySchemaDefinition> = LazyLock::new(build_ai_schema);
    &SCHEMA
}

/// `HARNESS_TELEMETRY_SCHEMA`.
pub fn harness_telemetry_schema() -> &'static TelemetrySchemaDefinition {
    static SCHEMA: LazyLock<TelemetrySchemaDefinition> = LazyLock::new(build_harness_schema);
    &SCHEMA
}

/// `AGENT_TELEMETRY_SCHEMAS`: the AI schema followed by the harness schema.
pub fn agent_telemetry_schemas() -> [&'static TelemetrySchemaDefinition; 2] {
    [ai_telemetry_schema(), harness_telemetry_schema()]
}

/// `startAiSpan`: admits one AI request span through the telemetry context.
pub fn start_ai_span<'a, T, E, F, Fut>(
    telemetry_context: &'a dyn TelemetryContext,
    name: AiSpanName,
    attributes: SpanAttributes,
    callback: F,
) -> pi_telemetry::BoxFuture<'a, Result<T, E>>
where
    T: Send + 'static,
    E: Error + Send + 'static,
    F: FnOnce(TelemetrySpan) -> Fut + Send + 'a,
    Fut: Future<Output = Result<T, E>> + Send + 'a,
{
    start_span(telemetry_context, name.as_str(), attributes, callback)
}

/// `startHarnessSpan`: admits one harness span through the telemetry context.
pub fn start_harness_span<'a, T, E, F, Fut>(
    telemetry_context: &'a dyn TelemetryContext,
    name: HarnessSpanName,
    attributes: SpanAttributes,
    callback: F,
) -> pi_telemetry::BoxFuture<'a, Result<T, E>>
where
    T: Send + 'static,
    E: Error + Send + 'static,
    F: FnOnce(TelemetrySpan) -> Fut + Send + 'a,
    Fut: Future<Output = Result<T, E>> + Send + 'a,
{
    start_span(telemetry_context, name.as_str(), attributes, callback)
}

fn start_span<'a, T, E, F, Fut>(
    telemetry_context: &'a dyn TelemetryContext,
    name: &str,
    attributes: SpanAttributes,
    callback: F,
) -> pi_telemetry::BoxFuture<'a, Result<T, E>>
where
    T: Send + 'static,
    E: Error + Send + 'static,
    F: FnOnce(TelemetrySpan) -> Fut + Send + 'a,
    Fut: Future<Output = Result<T, E>> + Send + 'a,
{
    telemetry_context.start_span(
        SpanOptions {
            name: name.to_owned(),
            attributes: Some(attributes),
        },
        callback,
    )
}

fn build_ai_schema() -> TelemetrySchemaDefinition {
    let start = start_map(vec![
        (
            "pi.ai.operation",
            required(
                "Logical provider operation",
                string_values(&["stream", "fetch_deferred", "cancel_deferred", "generate_images"]),
            ),
        ),
        ("pi.ai.provider", required("Selected provider id", string())),
        ("pi.ai.model", required("Requested model id", string())),
        ("pi.ai.api", required("Provider API id", string())),
        (
            "pi.ai.streaming",
            required("Whether this operation returns a stream", boolean()),
        ),
        (
            "pi.ai.deferred",
            optional(
                "Whether the operation requests or participates in deferred execution",
                boolean(),
            ),
        ),
    ]);

    let end = end_map(vec![
        ("pi.ai.response.model", attribute("Concrete response model", string())),
        (
            "pi.ai.response.id",
            high_cardinality(attribute("Provider response id", string())),
        ),
        (
            "pi.ai.response.stop_reason",
            attribute(
                "Normalized terminal response reason",
                string_values(&["stop", "length", "tool_use", "error", "aborted", "deferred"]),
            ),
        ),
        ("pi.ai.http.status_code", attribute("Final HTTP status", number())),
        ("pi.ai.usage.input_tokens", attribute("Reported input tokens", number())),
        (
            "pi.ai.usage.output_tokens",
            attribute("Reported output tokens", number()),
        ),
        (
            "pi.ai.usage.cache_read_tokens",
            attribute("Reported cache-read tokens", number()),
        ),
        (
            "pi.ai.usage.cache_write_tokens",
            attribute("Reported cache-write tokens", number()),
        ),
        (
            "pi.ai.usage.reasoning_tokens",
            attribute("Reported reasoning tokens", number()),
        ),
        ("pi.ai.usage.total_tokens", attribute("Reported total tokens", number())),
        ("pi.ai.usage.cost", attribute("Reported total cost", number())),
        (
            "pi.ai.stream.chunk_count",
            attribute("Streamed update chunk count", number()),
        ),
        (
            "pi.ai.stream.time_to_first_chunk_ms",
            attribute("Elapsed milliseconds to first update chunk", number()),
        ),
        (
            "pi.ai.error.type",
            low_cardinality(attribute("Provider or transport error class", string())),
        ),
    ]);

    TelemetrySchemaDefinition {
        version: 1,
        spans: BTreeMap::from([(
            "pi.ai.request".to_owned(),
            span(
                "One logical request to an AI provider",
                TelemetryParentDefinition::Any,
                start,
                end,
                "The operation throws or returns an error result",
            ),
        )]),
    }
}

fn build_harness_schema() -> TelemetrySchemaDefinition {
    let mut spans = BTreeMap::new();
    spans.insert(
        "pi.harness.run".to_owned(),
        operation_span(
            "One admitted in-process run invocation",
            "run",
            "Run operation kind",
            &["completed", "aborted", "failed", "suspended"],
            "Run invocation outcome",
            "The run fails or throws",
        ),
    );
    spans.insert(
        "pi.harness.compaction".to_owned(),
        operation_span(
            "One admitted in-process manual compaction invocation",
            "compaction",
            "Compaction operation kind",
            &["completed", "declined", "aborted", "failed"],
            "Compaction invocation outcome",
            "The compaction fails or throws",
        ),
    );
    spans.insert(
        "pi.harness.navigation".to_owned(),
        operation_span(
            "One admitted in-process navigation invocation",
            "navigation",
            "Navigation operation kind",
            &["completed", "declined", "aborted", "failed"],
            "Navigation invocation outcome",
            "The navigation fails or throws",
        ),
    );
    spans.insert(
        "pi.harness.checkpoint".to_owned(),
        span(
            "One run checkpoint",
            TelemetryParentDefinition::Spans {
                spans: vec!["pi.harness.run".to_owned()],
            },
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                ("pi.operation.id", required_high("Durable operation id", string())),
                (
                    "pi.checkpoint.kind",
                    required(
                        "Checkpoint purpose",
                        string_values(&["normal", "failure_drain", "abort_reconcile"]),
                    ),
                ),
            ]),
            BTreeMap::new(),
            "Checkpoint work throws",
        ),
    );
    spans.insert(
        "pi.harness.turn".to_owned(),
        span(
            "One assistant response and its tool batch",
            TelemetryParentDefinition::Spans {
                spans: vec!["pi.harness.run".to_owned()],
            },
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                ("pi.operation.id", required_high("Durable operation id", string())),
                ("pi.turn.id", required_high("Invocation-local turn id", string())),
            ]),
            BTreeMap::new(),
            "Turn work throws",
        ),
    );
    spans.insert(
        "pi.harness.step".to_owned(),
        span(
            "One durable retry attempt",
            TelemetryParentDefinition::Spans {
                spans: vec![
                    "pi.harness.turn".to_owned(),
                    "pi.harness.checkpoint".to_owned(),
                    "pi.harness.compaction".to_owned(),
                    "pi.harness.navigation".to_owned(),
                ],
            },
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                ("pi.operation.id", required_high("Durable operation id", string())),
                (
                    "pi.step.kind",
                    required(
                        "Retryable step kind",
                        string_values(&["assistant", "compaction", "branch_summary"]),
                    ),
                ),
                (
                    "pi.step.attempt",
                    required("One-based durable attempt number", number()),
                ),
                (
                    "pi.compaction.reason",
                    optional(
                        "Compaction trigger",
                        string_values(&["manual", "threshold", "overflow"]),
                    ),
                ),
            ]),
            end_map(vec![(
                "pi.step.outcome",
                attribute(
                    "Attempt outcome",
                    string_values(&["succeeded", "retry", "failed", "aborted", "deferred", "overflow"]),
                ),
            )]),
            "The attempt retries, fails, or throws",
        ),
    );
    spans.insert(
        "pi.harness.tool".to_owned(),
        span(
            "One raw phase-2 tool execution",
            TelemetryParentDefinition::Spans {
                spans: vec!["pi.harness.turn".to_owned(), "pi.harness.run".to_owned()],
            },
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                ("pi.operation.id", required_high("Durable operation id", string())),
                ("pi.turn.id", optional_high("Invocation-local live turn id", string())),
                ("pi.tool.name", required("Tool name", string())),
                ("pi.tool.call_id", required_high("Tool call id", string())),
                (
                    "pi.tool.replay",
                    required("Declared replay policy", string_values(&["never", "safe"])),
                ),
                (
                    "pi.tool.recovery",
                    required("Whether this is recovery execution", boolean()),
                ),
            ]),
            end_map(vec![(
                "pi.tool.is_error",
                attribute("Whether raw phase-2 execution returned an error", boolean()),
            )]),
            "Raw phase-2 execution returns an error",
        ),
    );
    spans.insert(
        "pi.harness.hook".to_owned(),
        span(
            "One registered hook handler invocation",
            TelemetryParentDefinition::Any,
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                (
                    "pi.operation.id",
                    optional_high("Durable operation id when accepted", string()),
                ),
                (
                    "pi.hook.name",
                    required(
                        "Hook name",
                        string_values(&[
                            "before_run",
                            "before_resume",
                            "before_run_end",
                            "transform_context",
                            "before_request",
                            "before_payload",
                            "after_response",
                            "before_tool",
                            "after_tool",
                            "before_compaction",
                            "before_navigation",
                        ]),
                    ),
                ),
                (
                    "pi.hook.registration_id",
                    optional("Stable hook registration id", string()),
                ),
            ]),
            end_map(vec![(
                "pi.hook.outcome",
                attribute(
                    "Handler outcome",
                    string_values(&["completed", "skipped", "blocked", "failed"]),
                ),
            )]),
            "The handler throws",
        ),
    );
    spans.insert(
        "pi.harness.sleep".to_owned(),
        span(
            "One retry delay",
            TelemetryParentDefinition::Spans {
                spans: vec!["pi.harness.step".to_owned(), "pi.harness.run".to_owned()],
            },
            start_map(vec![
                ("pi.operation.id", required_high("Durable operation id", string())),
                (
                    "pi.sleep.delay_ms",
                    required("Requested delay in milliseconds", number()),
                ),
            ]),
            end_map(vec![(
                "pi.sleep.outcome",
                attribute("Delay outcome", string_values(&["elapsed", "aborted"])),
            )]),
            "Sleep work throws",
        ),
    );
    spans.insert(
        "pi.harness.event_handler".to_owned(),
        span(
            "One passive event listener invocation",
            TelemetryParentDefinition::Any,
            start_map(vec![
                (
                    "pi.event.type",
                    required_low(
                        "Delivered harness event type",
                        string_values(&[
                            "run_start",
                            "run_resume",
                            "run_suspend",
                            "run_abort",
                            "run_end",
                            "fault",
                            "handler_error",
                            "turn_start",
                            "turn_end",
                            "retry_scheduled",
                            "retry_start",
                            "retry_end",
                            "message_start",
                            "message_update",
                            "message_end",
                            "tool_start",
                            "tool_update",
                            "tool_end",
                            "entry_added",
                            "write_pending",
                            "queue_update",
                            "fact_update",
                            "config_update",
                            "compaction_start",
                            "compaction_end",
                            "navigation_start",
                            "navigation_end",
                            "lane_created",
                            "usage",
                        ]),
                    ),
                ),
                (
                    "pi.lane.name",
                    optional_high("Lane name for lane-scoped events", string()),
                ),
            ]),
            BTreeMap::new(),
            "The listener throws",
        ),
    );
    spans.insert(
        "pi.session.write".to_owned(),
        span(
            "One committed session mutation",
            TelemetryParentDefinition::Any,
            start_map(vec![
                ("pi.lane.name", required_high("Lane name", string())),
                (
                    "pi.operation.id",
                    optional_high("Durable operation id when accepted", string()),
                ),
                (
                    "pi.session.mutation",
                    required(
                        "Session mutation kind",
                        string_values(&["entry", "record", "lane", "fact"]),
                    ),
                ),
                (
                    "pi.session.item_type",
                    optional("Entry, record, lane, or fact subtype", string()),
                ),
            ]),
            end_map(vec![(
                "pi.session.seq",
                attribute("Committed session sequence when exposed", number()),
            )]),
            "Storage rejects the mutation",
        ),
    );

    TelemetrySchemaDefinition { version: 1, spans }
}

fn operation_span(
    description: &str,
    kind: &str,
    kind_description: &str,
    outcomes: &[&str],
    outcome_description: &str,
    error_when: &str,
) -> TelemetrySpanDefinition {
    let mut start = operation_start_attributes();
    start.insert(
        "pi.operation.kind".to_owned(),
        required(kind_description, string_values(&[kind])),
    );
    let mut end = end_map(vec![(
        "pi.operation.outcome",
        attribute(outcome_description, string_values(outcomes)),
    )]);
    end.extend(operation_error_attributes());
    span(
        description,
        TelemetryParentDefinition::RootOrExternal,
        start,
        end,
        error_when,
    )
}

fn operation_start_attributes() -> BTreeMap<String, TelemetryRequiredAttributeDefinition> {
    start_map(vec![
        ("pi.session.id", required_high("Session id", string())),
        ("pi.lane.name", required_high("Lane name", string())),
        ("pi.operation.id", required_high("Durable operation id", string())),
        (
            "pi.operation.recovery",
            required("Whether this invocation resumes durable work", boolean()),
        ),
    ])
}

fn operation_error_attributes() -> BTreeMap<String, TelemetryAttributeDefinition> {
    end_map(vec![
        (
            "pi.error.code",
            low_cardinality(attribute("Stable operation error code", string())),
        ),
        (
            "pi.error.type",
            low_cardinality(attribute("Low-cardinality operation error class", string())),
        ),
    ])
}

fn span(
    description: &str,
    parents: TelemetryParentDefinition,
    start_attributes: BTreeMap<String, TelemetryRequiredAttributeDefinition>,
    end_attributes: BTreeMap<String, TelemetryAttributeDefinition>,
    error_when: &str,
) -> TelemetrySpanDefinition {
    TelemetrySpanDefinition {
        description: description.to_owned(),
        parents,
        start_attributes,
        end_attributes,
        events: None,
        status: TelemetrySpanStatusDefinition {
            default: TelemetryDefaultStatus::Ok,
            error_when: error_when.to_owned(),
        },
    }
}

fn attribute(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryAttributeDefinition {
    TelemetryAttributeDefinition {
        metadata: TelemetryAttributeMetadata {
            description: description.to_owned(),
            sensitive: None,
            cardinality: None,
        },
        value,
    }
}

fn high_cardinality(attribute: TelemetryAttributeDefinition) -> TelemetryAttributeDefinition {
    with_cardinality(attribute, TelemetryAttributeCardinality::High)
}

fn low_cardinality(attribute: TelemetryAttributeDefinition) -> TelemetryAttributeDefinition {
    with_cardinality(attribute, TelemetryAttributeCardinality::Low)
}

fn with_cardinality(
    mut attribute: TelemetryAttributeDefinition,
    cardinality: TelemetryAttributeCardinality,
) -> TelemetryAttributeDefinition {
    attribute.metadata.cardinality = Some(cardinality);
    attribute
}

fn required(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryRequiredAttributeDefinition {
    required_attribute(attribute(description, value), true)
}

fn optional(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryRequiredAttributeDefinition {
    required_attribute(attribute(description, value), false)
}

fn required_high(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryRequiredAttributeDefinition {
    required_attribute(high_cardinality(attribute(description, value)), true)
}

fn optional_high(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryRequiredAttributeDefinition {
    required_attribute(high_cardinality(attribute(description, value)), false)
}

fn required_low(description: &str, value: TelemetryAttributeValueDefinition) -> TelemetryRequiredAttributeDefinition {
    required_attribute(low_cardinality(attribute(description, value)), true)
}

fn required_attribute(attribute: TelemetryAttributeDefinition, required: bool) -> TelemetryRequiredAttributeDefinition {
    TelemetryRequiredAttributeDefinition { attribute, required }
}

fn string() -> TelemetryAttributeValueDefinition {
    TelemetryAttributeValueDefinition::String {
        values: None,
        examples: None,
    }
}

fn string_values(values: &[&str]) -> TelemetryAttributeValueDefinition {
    TelemetryAttributeValueDefinition::String {
        values: Some(values.iter().map(|value| (*value).to_owned()).collect()),
        examples: None,
    }
}

fn number() -> TelemetryAttributeValueDefinition {
    TelemetryAttributeValueDefinition::Number {
        values: None,
        examples: None,
    }
}

fn boolean() -> TelemetryAttributeValueDefinition {
    TelemetryAttributeValueDefinition::Boolean {
        values: None,
        examples: None,
    }
}

fn start_map(
    entries: Vec<(&str, TelemetryRequiredAttributeDefinition)>,
) -> BTreeMap<String, TelemetryRequiredAttributeDefinition> {
    entries
        .into_iter()
        .map(|(name, definition)| (name.to_owned(), definition))
        .collect()
}

fn end_map(entries: Vec<(&str, TelemetryAttributeDefinition)>) -> BTreeMap<String, TelemetryAttributeDefinition> {
    entries
        .into_iter()
        .map(|(name, definition)| (name.to_owned(), definition))
        .collect()
}
