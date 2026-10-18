use pi_telemetry::TelemetryMap;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures::channel::oneshot;
use futures::executor::block_on;
use pi_telemetry::{
    AttributeValue, InMemoryTelemetryContext, NOOP_TELEMETRY_CONTEXT, RecordedTelemetryEvent, SpanAttributes,
    SpanError, SpanOptions, SpanStatus, TelemetryAttributeDefinition, TelemetryAttributeMetadata,
    TelemetryAttributeValueDefinition, TelemetryContextExt, TelemetryParentDefinition, TelemetrySchemaDefinition,
    TelemetrySpan, TelemetrySpanDefinition, TelemetrySpanStatusDefinition, TelemetryStartAttributeDefinition,
    TypedSpanStarter, create_typed_span_starter, define_telemetry_schema,
};
use serde_json::json;

#[derive(Debug)]
struct Boom(&'static str);

impl fmt::Display for Boom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for Boom {}

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

fn string_attribute(description: &str, values: &[&str], required: bool) -> TelemetryStartAttributeDefinition {
    TelemetryStartAttributeDefinition {
        required,
        attribute: TelemetryAttributeDefinition {
            metadata: TelemetryAttributeMetadata {
                description: description.to_owned(),
                sensitive: None,
                cardinality: None,
            },
            value: TelemetryAttributeValueDefinition::String {
                values: Some(values.iter().map(|value| (*value).to_owned()).collect()),
                examples: None,
            },
        },
    }
}

fn sample_schema() -> TelemetrySchemaDefinition {
    TelemetrySchemaDefinition {
        version: 1,
        spans: TelemetryMap::from([(
            "operation".to_owned(),
            TelemetrySpanDefinition {
                description: "Test operation".to_owned(),
                parents: TelemetryParentDefinition::Any,
                start_attributes: TelemetryMap::from([(
                    "kind".to_owned(),
                    string_attribute("Kind", &["read", "write"], true),
                )]),
                end_attributes: TelemetryMap::new(),
                events: None,
                status: TelemetrySpanStatusDefinition {
                    default: pi_telemetry::TelemetryDefaultStatus::Ok,
                    error_when: "The operation fails".to_owned(),
                },
            },
        )]),
    }
}

#[test]
fn attribute_values_round_trip_through_their_wire_shape() {
    let values = [
        (AttributeValue::String("text".to_owned()), json!("text")),
        (AttributeValue::Number(1.5), json!(1.5)),
        (AttributeValue::Boolean(true), json!(true)),
        (
            AttributeValue::StringArray(vec!["a".to_owned(), "b".to_owned()]),
            json!(["a", "b"]),
        ),
        (AttributeValue::NumberArray(vec![1.0, 2.0]), json!([1.0, 2.0])),
        (AttributeValue::BooleanArray(vec![true, false]), json!([true, false])),
    ];

    for (value, expected) in values {
        assert_eq!(
            serde_json::to_value(&value).expect("attribute value must serialize"),
            expected
        );
        let roundtrip: AttributeValue = serde_json::from_value(expected).expect("attribute value must deserialize");
        assert_eq!(roundtrip, value);
    }
}

#[test]
fn preserves_serializable_schema_definitions() {
    let definition = sample_schema();
    let schema = define_telemetry_schema(definition.clone());

    assert_eq!(schema, definition);
    let json = serde_json::to_value(&schema).expect("schema must serialize");
    assert_eq!(json["version"], 1);
    assert_eq!(json["spans"]["operation"]["parents"]["kind"], "any");
    assert_eq!(json["spans"]["operation"]["startAttributes"]["kind"]["type"], "string");
    assert_eq!(json["spans"]["operation"]["startAttributes"]["kind"]["required"], true);
    assert_eq!(
        json["spans"]["operation"]["startAttributes"]["kind"]["values"][1],
        "write"
    );
    assert_eq!(json["spans"]["operation"]["status"]["default"], "ok");
    assert_eq!(json["spans"]["operation"]["status"]["errorWhen"], "The operation fails");

    let roundtrip: TelemetrySchemaDefinition = serde_json::from_value(json).expect("schema must deserialize");
    assert_eq!(roundtrip, schema);
}

#[test]
fn noop_admits_callbacks_synchronously_and_preserves_results() {
    let admitted = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_admitted = Arc::clone(&admitted);
    let callback_calls = Arc::clone(&calls);
    let future = NOOP_TELEMETRY_CONTEXT.start_span(options("success"), move |span| {
        callback_admitted.store(true, Ordering::SeqCst);
        callback_calls.fetch_add(1, Ordering::SeqCst);
        async move {
            let child = span
                .start_span(options("child"), |child_span| async move { Ok::<_, Boom>(child_span) })
                .await?;
            child.add_event("ignored", SpanAttributes::new());
            child.set_attributes(attributes(&[("ignored", AttributeValue::Boolean(true))]));
            child.set_status(SpanStatus::Error { error: None });
            Ok::<_, Boom>(42)
        }
    });

    assert!(admitted.load(Ordering::SeqCst));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(block_on(future).expect("noop spans never fail"), 42);
}

#[test]
fn noop_preserves_error_values() {
    let error =
        block_on(NOOP_TELEMETRY_CONTEXT.start_span(options("sync"), |_span| async { Err::<i32, _>(Boom("sync")) }))
            .expect_err("sync error must be preserved");
    assert_eq!(error.0, "sync");

    let error =
        block_on(NOOP_TELEMETRY_CONTEXT.start_span(options("async"), |_span| async { Err::<i32, _>(Boom("async")) }))
            .expect_err("async error must be preserved");
    assert_eq!(error.0, "async");
}

#[test]
fn memory_admits_once_synchronously_and_records_success() {
    let context = InMemoryTelemetryContext::new();
    let admitted = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_admitted = Arc::clone(&admitted);
    let callback_calls = Arc::clone(&calls);
    let future = context.start_span(options("success"), move |_span| {
        callback_admitted.store(true, Ordering::SeqCst);
        callback_calls.fetch_add(1, Ordering::SeqCst);
        async move { Ok::<_, Boom>(42) }
    });

    assert!(admitted.load(Ordering::SeqCst));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(block_on(future).expect("successful span"), 42);

    let spans = context.get_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "success");
    assert_eq!(spans[0].status, SpanStatus::Ok);
    assert!(spans[0].settled);
    assert_eq!(spans[0].end_sequence, Some(1));
}

#[test]
fn memory_preserves_error_values_and_records_automatic_error_status() {
    let context = InMemoryTelemetryContext::new();
    let error = block_on(context.start_span(options("sync-error"), |_span| async { Err::<i32, _>(Boom("sync")) }))
        .expect_err("sync error must be preserved");
    assert_eq!(error.0, "sync");

    let error = block_on(context.start_span(options("async-error"), |_span| async { Err::<i32, _>(Boom("async")) }))
        .expect_err("async error must be preserved");
    assert_eq!(error.0, "async");

    let spans = context.get_spans();
    for span in &spans {
        match &span.status {
            SpanStatus::Error { error: Some(error) } => {
                assert_eq!(error.name, "Boom");
                assert!(error.message == "sync" || error.message == "async");
            }
            status => panic!("expected automatic error status with details, got {status:?}"),
        }
        assert!(span.settled);
    }
}

#[test]
fn memory_uses_last_explicit_status_without_automatic_overwrite() {
    let context = InMemoryTelemetryContext::new();
    block_on(context.start_span(options("last-status"), |span| async move {
        span.set_status(SpanStatus::Error {
            error: Some(SpanError {
                name: "Expected".to_owned(),
                message: "first".to_owned(),
            }),
        });
        span.set_status(SpanStatus::Ok);
        Ok::<_, Boom>(())
    }))
    .expect("span");

    block_on(
        context.start_span(options("explicit-before-failure"), |span| async move {
            span.set_status(SpanStatus::Ok);
            Err::<(), _>(Boom("after explicit status"))
        }),
    )
    .expect_err("span must fail");

    block_on(context.start_span(options("expected-failure"), |span| async move {
        span.set_status(SpanStatus::Error {
            error: Some(SpanError {
                name: "Expected".to_owned(),
                message: "returned failure".to_owned(),
            }),
        });
        Ok::<_, Boom>(())
    }))
    .expect("span");

    let spans = context.get_spans();
    assert_eq!(spans[0].status, SpanStatus::Ok);
    assert_eq!(spans[1].status, SpanStatus::Ok);
    assert_eq!(
        spans[2].status,
        SpanStatus::Error {
            error: Some(SpanError {
                name: "Expected".to_owned(),
                message: "returned failure".to_owned()
            })
        }
    );
}

#[test]
fn memory_merges_attributes_and_records_ordered_events() {
    let context = InMemoryTelemetryContext::new();
    block_on(context.start_span(
        SpanOptions {
            name: "recording".to_owned(),
            attributes: Some(attributes(&[
                ("start", AttributeValue::String("value".to_owned())),
                ("overwrite", AttributeValue::String("start".to_owned())),
            ])),
        },
        |span| async move {
            span.set_attributes(attributes(&[
                ("count", AttributeValue::Number(1.0)),
                ("overwrite", AttributeValue::String("middle".to_owned())),
            ]));
            span.set_attributes(attributes(&[("overwrite", AttributeValue::String("end".to_owned()))]));
            span.add_event("first", attributes(&[("index", AttributeValue::Number(1.0))]));
            span.add_event("second", attributes(&[("index", AttributeValue::Number(2.0))]));
            Ok::<_, Boom>(())
        },
    ))
    .expect("span");

    let spans = context.get_spans();
    assert_eq!(
        spans[0].attributes,
        attributes(&[
            ("start", AttributeValue::String("value".to_owned())),
            ("overwrite", AttributeValue::String("end".to_owned())),
            ("count", AttributeValue::Number(1.0)),
        ])
    );
    assert_eq!(
        spans[0].events,
        vec![
            RecordedTelemetryEvent {
                name: "first".to_owned(),
                attributes: attributes(&[("index", AttributeValue::Number(1.0))]),
            },
            RecordedTelemetryEvent {
                name: "second".to_owned(),
                attributes: attributes(&[("index", AttributeValue::Number(2.0))]),
            },
        ]
    );
}

#[test]
fn memory_makes_calls_after_settlement_inert() {
    let context = InMemoryTelemetryContext::new();
    let mut settled_span: Option<TelemetrySpan> = None;
    block_on(context.start_span(
        SpanOptions {
            name: "settled".to_owned(),
            attributes: Some(attributes(&[("value", AttributeValue::String("initial".to_owned()))])),
        },
        |span| {
            settled_span = Some(span);
            async move { Ok::<_, Boom>(()) }
        },
    ))
    .expect("span");

    let span = settled_span.expect("callback span");
    span.set_attributes(attributes(&[("value", AttributeValue::String("late".to_owned()))]));
    span.add_event("late", attributes(&[("value", AttributeValue::Boolean(true))]));
    span.set_status(SpanStatus::Error { error: None });

    let mut child_admitted = false;
    let child = block_on(span.start_span(options("late-child"), |_child| {
        child_admitted = true;
        async move { Ok::<_, Boom>(7) }
    }));
    assert!(child_admitted);
    assert_eq!(child.expect("late child still runs"), 7);

    let spans = context.get_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].attributes,
        attributes(&[("value", AttributeValue::String("initial".to_owned()))])
    );
    assert!(spans[0].events.is_empty());
    assert_eq!(spans[0].status, SpanStatus::Ok);
}

#[test]
fn memory_records_nested_and_concurrent_child_relationships() {
    let context = InMemoryTelemetryContext::new();
    block_on(context.start_span(options("parent"), |parent| async move {
        let (release_first, first_gate) = oneshot::channel::<()>();
        let first = parent.start_span(options("first-child"), move |_child| async move {
            let _ = first_gate.await;
            Ok::<_, Boom>(())
        });
        let second = parent.start_span(options("second-child"), |_child| async move { Ok::<_, Boom>("done") });
        assert_eq!(second.await.expect("second child"), "done");
        release_first.send(()).expect("release first child");
        first.await.expect("first child");
        Ok::<_, Boom>(())
    }))
    .expect("parent span");

    let spans = context.get_spans();
    let parent = spans.iter().find(|span| span.name == "parent").expect("parent span");
    let first = spans
        .iter()
        .find(|span| span.name == "first-child")
        .expect("first child");
    let second = spans
        .iter()
        .find(|span| span.name == "second-child")
        .expect("second child");

    assert_eq!(parent.parent_id, None);
    assert_eq!(first.parent_id, Some(parent.id));
    assert_eq!(second.parent_id, Some(parent.id));
    let first_end = first.end_sequence.expect("first child settled");
    let second_end = second.end_sequence.expect("second child settled");
    let parent_end = parent.end_sequence.expect("parent settled");
    assert!(second_end < first_end);
    assert!(first_end < parent_end);
}

#[test]
fn memory_returns_detached_snapshots() {
    let context = InMemoryTelemetryContext::new();
    let mut open_settled = None;
    let mut open_end_sequence = None;
    block_on(context.start_span(
        SpanOptions {
            name: "snapshot".to_owned(),
            attributes: Some(attributes(&[(
                "tags",
                AttributeValue::StringArray(vec!["initial".to_owned()]),
            )])),
        },
        |span| {
            span.add_event("event", attributes(&[("value", AttributeValue::Number(1.0))]));
            let spans = context.get_spans();
            open_settled = Some(spans[0].settled);
            open_end_sequence = spans[0].end_sequence;
            async move { Ok::<_, Boom>(()) }
        },
    ))
    .expect("span");

    assert_eq!(open_settled, Some(false));
    assert_eq!(open_end_sequence, None);

    let first = context.get_spans();
    assert!(first[0].settled);
    assert_eq!(first[0].end_sequence, Some(1));
    let mut mutated = first[0].clone();
    mutated.attributes.insert(
        "tags".to_owned(),
        AttributeValue::StringArray(vec!["mutated".to_owned()]),
    );
    mutated.events[0]
        .attributes
        .insert("value".to_owned(), AttributeValue::Number(2.0));

    let second = context.get_spans();
    assert_eq!(
        second[0].attributes,
        attributes(&[("tags", AttributeValue::StringArray(vec!["initial".to_owned()]))])
    );
    assert_eq!(
        second[0].events,
        vec![RecordedTelemetryEvent {
            name: "event".to_owned(),
            attributes: attributes(&[("value", AttributeValue::Number(1.0))]),
        }]
    );
}

#[test]
fn typed_span_starter_binds_child_starters_to_their_parent_spans() {
    let request_schema = define_telemetry_schema(TelemetrySchemaDefinition {
        version: 3,
        spans: TelemetryMap::from([(
            "request".to_owned(),
            TelemetrySpanDefinition {
                description: "Request".to_owned(),
                parents: TelemetryParentDefinition::Spans {
                    spans: vec!["operation".to_owned()],
                },
                start_attributes: TelemetryMap::from([(
                    "provider".to_owned(),
                    string_attribute("Provider", &[], true),
                )]),
                end_attributes: TelemetryMap::new(),
                events: None,
                status: TelemetrySpanStatusDefinition {
                    default: pi_telemetry::TelemetryDefaultStatus::Ok,
                    error_when: "The request fails".to_owned(),
                },
            },
        )]),
    });
    let telemetry = InMemoryTelemetryContext::new();
    let start_span: TypedSpanStarter<&InMemoryTelemetryContext> =
        create_typed_span_starter(&telemetry, &[sample_schema(), request_schema]);

    let result = block_on(start_span.start_span(
        "operation",
        attributes(&[("kind", AttributeValue::String("read".to_owned()))]),
        |_operation_span, start_child_span| async move {
            start_child_span
                .start_span(
                    "request",
                    attributes(&[("provider", AttributeValue::String("example".to_owned()))]),
                    |request_span, _request_children| async move {
                        request_span
                            .set_attributes(attributes(&[("response", AttributeValue::String("cached".to_owned()))]));
                        Ok::<_, Boom>(42)
                    },
                )
                .await
        },
    ))
    .expect("typed span chain");
    assert_eq!(result, 42);

    let spans = telemetry.get_spans();
    let operation = spans
        .iter()
        .find(|span| span.name == "operation")
        .expect("operation span");
    let request = spans.iter().find(|span| span.name == "request").expect("request span");
    assert_eq!(operation.parent_id, None);
    assert_eq!(request.parent_id, Some(operation.id));

    let error = block_on(start_span.start_span(
        "operation",
        attributes(&[("kind", AttributeValue::String("write".to_owned()))]),
        |_span, _children| async move { Err::<(), _>(Boom("sync")) },
    ))
    .expect_err("typed starter must preserve errors");
    assert_eq!(error.0, "sync");
}
