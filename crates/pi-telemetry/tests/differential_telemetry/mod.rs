#[path = "../support/oracle.rs"]
mod oracle;

use pi_telemetry::TelemetryMap;
use std::error::Error;
use std::fmt;

use futures::executor::block_on;
use pi_telemetry::{
    AttributeValue, InMemoryTelemetryContext, SpanAttributes, SpanError, SpanOptions, SpanStatus,
    TelemetryAttributeDefinition, TelemetryAttributeMetadata, TelemetryAttributeValueDefinition, TelemetryContextExt,
    TelemetryEventDefinition, TelemetryParentDefinition, TelemetrySchemaDefinition, TelemetrySpan,
    TelemetrySpanDefinition, TelemetrySpanStatusDefinition, TelemetryStartAttributeDefinition, define_telemetry_schema,
};
use serde_json::{Value, json};

#[derive(Debug)]
struct Failure;

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("failure")
    }
}

impl Error for Failure {}

fn options(name: &str) -> SpanOptions {
    SpanOptions {
        name: name.to_owned(),
        attributes: None,
    }
}

fn attributes(pairs: &[(&str, AttributeValue)]) -> SpanAttributes {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn string(value: &str) -> AttributeValue {
    AttributeValue::String(value.to_owned())
}

fn number(value: f64) -> AttributeValue {
    AttributeValue::Number(value)
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(number) => Value::from(number.as_f64().expect("telemetry numbers must be finite")),
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(name, value)| (name.clone(), normalize(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn request(oracle: &mut oracle::TsOracle, scenario: &str) -> Value {
    let response = oracle.request(&json!({ "id": scenario, "scenario": scenario }));
    assert_eq!(response["ok"], true, "oracle scenario {scenario} failed: {response}");
    response["output"].clone()
}

fn assert_spans_match(oracle: &mut oracle::TsOracle, scenario: &str, context: &InMemoryTelemetryContext) {
    let expected = normalize(&request(oracle, scenario)["spans"]);
    let actual = normalize(&serde_json::to_value(context.get_spans()).expect("spans must serialize"));
    assert_eq!(actual, expected, "scenario {scenario}");
}

fn run_recording(context: &InMemoryTelemetryContext) {
    block_on(context.start_span(
        SpanOptions {
            name: "recording".to_owned(),
            attributes: Some(attributes(&[
                ("start", string("value")),
                ("overwrite", string("start")),
            ])),
        },
        |span| async move {
            span.set_attributes(attributes(&[("count", number(1.0)), ("overwrite", string("middle"))]));
            span.set_attributes(attributes(&[("overwrite", string("end"))]));
            span.add_event("first", attributes(&[("index", number(1.0))]));
            span.add_event("second", attributes(&[("index", number(2.0))]));
            Ok::<_, Failure>(())
        },
    ))
    .expect("recording span");
}

fn run_status(context: &InMemoryTelemetryContext) {
    block_on(context.start_span(options("last-status"), |span| async move {
        span.set_status(SpanStatus::Error {
            error: Some(SpanError {
                name: "Expected".to_owned(),
                message: "first".to_owned(),
            }),
        });
        span.set_status(SpanStatus::Ok);
        Ok::<_, Failure>(())
    }))
    .expect("last-status span");
    block_on(context.start_span(options("expected-failure"), |span| async move {
        span.set_status(SpanStatus::Error {
            error: Some(SpanError {
                name: "Expected".to_owned(),
                message: "returned failure".to_owned(),
            }),
        });
        Ok::<_, Failure>(())
    }))
    .expect("expected-failure span");
}

fn run_parentage(context: &InMemoryTelemetryContext) {
    block_on(context.start_span(options("parent"), |parent| async move {
        parent
            .start_span(options("first-child"), |_child| async move { Ok::<_, Failure>(()) })
            .await?;
        parent
            .start_span(options("second-child"), |_child| async move { Ok::<_, Failure>(()) })
            .await?;
        Ok::<_, Failure>(())
    }))
    .expect("parent span");
}

fn run_settled(context: &InMemoryTelemetryContext) {
    let mut settled_span: Option<TelemetrySpan> = None;
    block_on(context.start_span(
        SpanOptions {
            name: "settled".to_owned(),
            attributes: Some(attributes(&[("value", string("initial"))])),
        },
        |span| {
            settled_span = Some(span);
            async move { Ok::<_, Failure>(()) }
        },
    ))
    .expect("settled span");

    let span = settled_span.expect("callback span");
    span.set_attributes(attributes(&[("value", string("late"))]));
    span.add_event("late", attributes(&[("value", AttributeValue::Boolean(true))]));
    span.set_status(SpanStatus::Error { error: None });
    let child = block_on(span.start_span(options("late-child"), |_child| async move { Ok::<_, Failure>(7) }));
    assert_eq!(child.expect("late child still runs"), 7);
}

fn differential_schema() -> TelemetrySchemaDefinition {
    define_telemetry_schema(TelemetrySchemaDefinition {
        version: 1,
        spans: TelemetryMap::from([(
            "operation".to_owned(),
            TelemetrySpanDefinition {
                description: "Test operation".to_owned(),
                parents: TelemetryParentDefinition::Any,
                start_attributes: TelemetryMap::from([(
                    "kind".to_owned(),
                    TelemetryStartAttributeDefinition {
                        required: true,
                        attribute: TelemetryAttributeDefinition {
                            metadata: TelemetryAttributeMetadata {
                                description: "Kind".to_owned(),
                                sensitive: None,
                                cardinality: None,
                            },
                            value: TelemetryAttributeValueDefinition::String {
                                values: Some(vec!["read".to_owned(), "write".to_owned()]),
                                examples: None,
                            },
                        },
                    },
                )]),
                end_attributes: TelemetryMap::new(),
                events: Some(TelemetryMap::from([(
                    "result".to_owned(),
                    TelemetryEventDefinition {
                        description: "Result".to_owned(),
                        attributes: TelemetryMap::from([(
                            "outcome".to_owned(),
                            TelemetryStartAttributeDefinition {
                                required: true,
                                attribute: TelemetryAttributeDefinition {
                                    metadata: TelemetryAttributeMetadata {
                                        description: "Outcome".to_owned(),
                                        sensitive: None,
                                        cardinality: None,
                                    },
                                    value: TelemetryAttributeValueDefinition::String {
                                        values: Some(vec!["ok".to_owned(), "error".to_owned()]),
                                        examples: None,
                                    },
                                },
                            },
                        )]),
                    },
                )])),
                status: TelemetrySpanStatusDefinition {
                    default: pi_telemetry::TelemetryDefaultStatus::Ok,
                    error_when: "The operation fails".to_owned(),
                },
            },
        )]),
    })
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("telemetry-oracle.ts", "packages/telemetry/src/index.ts");

    let recording = InMemoryTelemetryContext::new();
    run_recording(&recording);
    assert_spans_match(&mut oracle, "recording", &recording);

    let status = InMemoryTelemetryContext::new();
    run_status(&status);
    assert_spans_match(&mut oracle, "status", &status);

    let parentage = InMemoryTelemetryContext::new();
    run_parentage(&parentage);
    assert_spans_match(&mut oracle, "parentage", &parentage);

    let settled = InMemoryTelemetryContext::new();
    run_settled(&settled);
    assert_spans_match(&mut oracle, "settled", &settled);

    let expected_schema = normalize(&request(&mut oracle, "schema")["schema"]);
    let actual_schema = normalize(&serde_json::to_value(differential_schema()).expect("schema must serialize"));
    assert_eq!(actual_schema, expected_schema, "schema");
}
