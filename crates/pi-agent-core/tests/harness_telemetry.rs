use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_agent_core::harness::telemetry::{
    AiSpanName, HarnessSpanName, agent_telemetry_schemas, ai_telemetry_schema, harness_telemetry_schema, start_ai_span,
    start_harness_span,
};
use pi_telemetry::{
    AttributeValue, InMemoryTelemetryContext, SpanAttributes, SpanStatus, TelemetryAttributeCardinality,
    TelemetryAttributeValueDefinition, TelemetryParentDefinition,
};

#[derive(Debug)]
struct Failure;

impl Display for Failure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("failure")
    }
}

impl Error for Failure {}

fn attributes(pairs: &[(&str, AttributeValue)]) -> SpanAttributes {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn string(value: &str) -> AttributeValue {
    AttributeValue::String(value.to_owned())
}

fn boolean(value: bool) -> AttributeValue {
    AttributeValue::Boolean(value)
}

#[test]
fn declares_the_ai_request_schema() {
    let schema = ai_telemetry_schema();
    assert_eq!(schema.version, 1);
    assert_eq!(schema.spans.keys().collect::<Vec<_>>(), vec!["pi.ai.request"]);
    let span = &schema.spans["pi.ai.request"];
    assert_eq!(span.description, "One logical request to an AI provider");
    assert!(matches!(span.parents, TelemetryParentDefinition::Any));
    assert_eq!(
        span.status.error_when,
        "The operation throws or returns an error result"
    );

    let operation = &span.start_attributes["pi.ai.operation"];
    assert!(operation.required);
    assert_eq!(
        operation.attribute.value,
        TelemetryAttributeValueDefinition::String {
            values: Some(vec![
                "stream".to_owned(),
                "fetch_deferred".to_owned(),
                "cancel_deferred".to_owned(),
                "generate_images".to_owned()
            ]),
            examples: None,
        }
    );
    assert!(!span.start_attributes["pi.ai.deferred"].required);
    assert_eq!(
        span.end_attributes["pi.ai.response.id"].metadata.cardinality,
        Some(TelemetryAttributeCardinality::High)
    );
    assert_eq!(
        span.end_attributes["pi.ai.response.stop_reason"].value,
        TelemetryAttributeValueDefinition::String {
            values: Some(vec![
                "stop".to_owned(),
                "length".to_owned(),
                "tool_use".to_owned(),
                "error".to_owned(),
                "aborted".to_owned(),
                "deferred".to_owned()
            ]),
            examples: None,
        }
    );
    assert!(span.events.is_none());
}

#[test]
fn declares_every_harness_span_in_the_source_order_and_combined_schema_order() {
    let schema = harness_telemetry_schema();
    let names: Vec<&str> = schema.spans.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "pi.harness.checkpoint",
            "pi.harness.compaction",
            "pi.harness.event_handler",
            "pi.harness.hook",
            "pi.harness.navigation",
            "pi.harness.run",
            "pi.harness.sleep",
            "pi.harness.step",
            "pi.harness.tool",
            "pi.harness.turn",
            "pi.session.write",
        ]
    );
    let schemas = agent_telemetry_schemas();
    assert_eq!(schemas.len(), 2);
    assert_eq!(schemas[0].spans.len(), 1);
    assert_eq!(schemas[1].spans.len(), 11);

    let step = &schema.spans["pi.harness.step"];
    assert_eq!(
        step.parents,
        TelemetryParentDefinition::Spans {
            spans: vec![
                "pi.harness.turn".to_owned(),
                "pi.harness.checkpoint".to_owned(),
                "pi.harness.compaction".to_owned(),
                "pi.harness.navigation".to_owned()
            ]
        }
    );
    let kind = &step.start_attributes["pi.step.kind"];
    assert!(kind.required);
    assert_eq!(
        kind.attribute.value,
        TelemetryAttributeValueDefinition::String {
            values: Some(vec![
                "assistant".to_owned(),
                "compaction".to_owned(),
                "branch_summary".to_owned()
            ]),
            examples: None,
        }
    );
    assert!(step.end_attributes["pi.step.outcome"].metadata.cardinality.is_none());
}

#[tokio::test]
async fn records_ai_and_harness_spans_through_the_composed_context() {
    let context = InMemoryTelemetryContext::new();

    start_ai_span(
        &context,
        AiSpanName::PiAiRequest,
        attributes(&[
            ("pi.ai.operation", string("stream")),
            ("pi.ai.provider", string("provider")),
            ("pi.ai.model", string("model")),
            ("pi.ai.api", string("api")),
            ("pi.ai.streaming", boolean(true)),
        ]),
        |span| async move {
            span.set_attributes(attributes(&[("pi.ai.response.stop_reason", string("tool_use"))]));
            Ok::<_, Failure>(())
        },
    )
    .await
    .expect("ai span");

    start_harness_span(
        &context,
        HarnessSpanName::PiHarnessRun,
        attributes(&[
            ("pi.session.id", string("session")),
            ("pi.lane.name", string("main")),
            ("pi.operation.id", string("operation")),
            ("pi.operation.kind", string("run")),
            ("pi.operation.recovery", boolean(false)),
        ]),
        |span| async move {
            span.set_attributes(attributes(&[("pi.operation.outcome", string("completed"))]));
            Ok::<_, Failure>(())
        },
    )
    .await
    .expect("harness span");

    let spans = context.get_spans();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].name, "pi.ai.request");
    assert_eq!(spans[0].attributes["pi.ai.operation"], string("stream"));
    assert_eq!(spans[0].attributes["pi.ai.response.stop_reason"], string("tool_use"));
    assert_eq!(spans[0].status, SpanStatus::Ok);
    assert_eq!(spans[1].name, "pi.harness.run");
    assert_eq!(spans[1].attributes["pi.operation.outcome"], string("completed"));
    assert_eq!(spans[1].status, SpanStatus::Ok);
}

#[tokio::test]
async fn maps_callback_failures_to_error_status() {
    let context = InMemoryTelemetryContext::new();
    let result: Result<(), Failure> = start_harness_span(
        &context,
        HarnessSpanName::PiHarnessCheckpoint,
        attributes(&[
            ("pi.lane.name", string("main")),
            ("pi.operation.id", string("operation")),
            ("pi.checkpoint.kind", string("normal")),
        ]),
        |_span| async { Err(Failure) },
    )
    .await;
    assert!(result.is_err());
    let spans = context.get_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].status,
        SpanStatus::Error {
            error: Some(pi_telemetry::SpanError {
                name: "Failure".to_owned(),
                message: "failure".to_owned(),
            })
        }
    );
}

#[test]
fn span_names_expose_the_source_literals() {
    assert_eq!(AiSpanName::PiAiRequest.as_str(), "pi.ai.request");
    let names = [
        HarnessSpanName::PiHarnessRun,
        HarnessSpanName::PiHarnessCompaction,
        HarnessSpanName::PiHarnessNavigation,
        HarnessSpanName::PiHarnessCheckpoint,
        HarnessSpanName::PiHarnessTurn,
        HarnessSpanName::PiHarnessStep,
        HarnessSpanName::PiHarnessTool,
        HarnessSpanName::PiHarnessHook,
        HarnessSpanName::PiHarnessSleep,
        HarnessSpanName::PiHarnessEventHandler,
        HarnessSpanName::PiSessionWrite,
    ];
    assert_eq!(
        names.map(HarnessSpanName::as_str),
        [
            "pi.harness.run",
            "pi.harness.compaction",
            "pi.harness.navigation",
            "pi.harness.checkpoint",
            "pi.harness.turn",
            "pi.harness.step",
            "pi.harness.tool",
            "pi.harness.hook",
            "pi.harness.sleep",
            "pi.harness.event_handler",
            "pi.session.write",
        ]
    );
}
