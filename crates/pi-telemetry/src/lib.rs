//! Rust port of Pi's telemetry primitives.

use indexmap::IndexMap;
use std::any::{Any, type_name};
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod memory;
mod noop;

pub use memory::{InMemoryTelemetryContext, RecordedTelemetryEvent, RecordedTelemetrySpan};
pub use noop::{NOOP_TELEMETRY_CONTEXT, NoopTelemetryContext};

/// One attribute value accepted by the telemetry contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
    StringArray(Vec<String>),
    NumberArray(Vec<f64>),
    BooleanArray(Vec<bool>),
}

/// Span attributes keyed by attribute name. JavaScript `undefined` entries are not representable.
/// String-keyed map that keeps insertion order, like a TypeScript object literal.
pub type TelemetryMap<V> = IndexMap<String, V>;

pub type SpanAttributes = TelemetryMap<AttributeValue>;

/// Options admitted by one span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanOptions {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<SpanAttributes>,
}

/// Structured error details carried by an error status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanError {
    pub name: String,
    pub message: String,
}

/// Terminal or explicit status of a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum SpanStatus {
    Ok,
    Error {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<SpanError>,
    },
}

/// A boxed future that is sendable across the telemetry boundary.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Callback admitted by the dyn-safe telemetry contract.
pub type ErasedSpanCallback<'a> = Box<dyn FnOnce(TelemetrySpan) -> BoxFuture<'a, SpanOutcome> + Send + 'a>;

/// Erased callback outcome. The typed value is downcast by [`TelemetryContextExt::start_span`].
pub struct SpanOutcome {
    value: Box<dyn Any + Send>,
    error: Option<SpanError>,
}

impl SpanOutcome {
    pub(crate) fn new(value: Box<dyn Any + Send>, error: Option<SpanError>) -> Self {
        Self { value, error }
    }

    /// Returns the error details used for automatic error status, or `None` for success.
    pub fn error(&self) -> Option<&SpanError> {
        self.error.as_ref()
    }
}

/// Dyn-safe telemetry backend contract. Applications normally call [`TelemetryContextExt::start_span`].
pub trait TelemetryContext: Send + Sync {
    fn start_span_erased<'a>(
        &'a self,
        options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome>;
}

/// Typed span admission on top of the erased backend contract.
///
/// JavaScript exceptions map to `Result::Err`; an `Err` automatically records an error status unless an
/// explicit status was set.
pub trait TelemetryContextExt: TelemetryContext {
    fn start_span<'a, T, E, F, Fut>(&'a self, options: SpanOptions, callback: F) -> BoxFuture<'a, Result<T, E>>
    where
        T: Send + 'static,
        E: Error + Send + 'static,
        F: FnOnce(TelemetrySpan) -> Fut + Send + 'a,
        Fut: Future<Output = Result<T, E>> + Send + 'a,
    {
        let outcome = self.start_span_erased(
            options,
            Box::new(move |span| {
                let result = callback(span);
                Box::pin(async move {
                    let result = result.await;
                    let error = result.as_ref().err().map(span_error);
                    SpanOutcome::new(Box::new(result), error)
                })
            }),
        );

        Box::pin(async move {
            let outcome = outcome.await;
            *outcome
                .value
                .downcast::<Result<T, E>>()
                .expect("telemetry callback outcome must match its erased type")
        })
    }
}

impl<C: TelemetryContext + ?Sized> TelemetryContextExt for C {}

impl<T: TelemetryContext + ?Sized> TelemetryContext for &T {
    fn start_span_erased<'a>(
        &'a self,
        options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome> {
        (*self).start_span_erased(options, callback)
    }
}

impl<T: TelemetryContext + ?Sized> TelemetryContext for Arc<T> {
    fn start_span_erased<'a>(
        &'a self,
        options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome> {
        self.as_ref().start_span_erased(options, callback)
    }
}

fn span_error<E: Error>(error: &E) -> SpanError {
    SpanError {
        name: short_type_name::<E>(),
        message: error.to_string(),
    }
}

fn short_type_name<T: ?Sized>() -> String {
    type_name::<T>()
        .rsplit("::")
        .next()
        .expect("type names are non-empty")
        .to_owned()
}

/// A recorded span or the shared inert span.
#[derive(Clone)]
pub struct TelemetrySpan {
    inner: SpanInner,
}

#[derive(Clone)]
enum SpanInner {
    Noop,
    InMemory(memory::InMemorySpanHandle),
}

impl TelemetrySpan {
    pub(crate) fn noop() -> Self {
        Self { inner: SpanInner::Noop }
    }

    pub(crate) fn in_memory(handle: memory::InMemorySpanHandle) -> Self {
        Self {
            inner: SpanInner::InMemory(handle),
        }
    }

    pub fn add_event(&self, name: impl Into<String>, attributes: impl Into<Option<SpanAttributes>>) {
        match &self.inner {
            SpanInner::Noop => {}
            SpanInner::InMemory(handle) => handle.add_event(name.into(), attributes.into()),
        }
    }

    pub fn set_attributes(&self, attributes: SpanAttributes) {
        match &self.inner {
            SpanInner::Noop => {}
            SpanInner::InMemory(handle) => handle.set_attributes(attributes),
        }
    }

    pub fn set_status(&self, status: SpanStatus) {
        match &self.inner {
            SpanInner::Noop => {}
            SpanInner::InMemory(handle) => handle.set_status(status),
        }
    }
}

impl TelemetryContext for TelemetrySpan {
    fn start_span_erased<'a>(
        &'a self,
        options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome> {
        match &self.inner {
            SpanInner::Noop => callback(TelemetrySpan::noop()),
            SpanInner::InMemory(handle) => memory::start_in_memory_span(&handle.state, Some(handle), options, callback),
        }
    }
}

/// Binds an explicit parent context to a span vocabulary.
///
/// Schema values are accepted for source parity but are not inspected at runtime.
pub struct TypedSpanStarter<C> {
    context: C,
}

impl<C: TelemetryContext> TypedSpanStarter<C> {
    pub fn new(context: C) -> Self {
        Self { context }
    }

    pub fn start_span<'a, T, E, F, Fut>(
        &'a self,
        name: impl Into<String>,
        attributes: SpanAttributes,
        callback: F,
    ) -> BoxFuture<'a, Result<T, E>>
    where
        T: Send + 'static,
        E: Error + Send + 'static,
        F: FnOnce(TelemetrySpan, TypedSpanStarter<TelemetrySpan>) -> Fut + Send + 'a,
        Fut: Future<Output = Result<T, E>> + Send + 'a,
    {
        let options = SpanOptions {
            name: name.into(),
            attributes: Some(attributes),
        };
        self.context.start_span(options, move |span| {
            let child_starter = TypedSpanStarter::new(span.clone());
            callback(span, child_starter)
        })
    }
}

/// Creates a starter bound to `context`. Schema values are used only for TypeScript type inference.
pub fn create_typed_span_starter<C: TelemetryContext>(
    context: C,
    _schemas: &[TelemetrySchemaDefinition],
) -> TypedSpanStarter<C> {
    TypedSpanStarter::new(context)
}

/// Attribute value type names used by schema definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelemetryAttributeType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "number")]
    Number,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "string[]")]
    StringArray,
    #[serde(rename = "number[]")]
    NumberArray,
    #[serde(rename = "boolean[]")]
    BooleanArray,
}

/// Attribute cardinality hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryAttributeCardinality {
    Low,
    High,
}

/// Common schema metadata shared by every attribute definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryAttributeMetadata {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<TelemetryAttributeCardinality>,
}

/// Type-specific schema value constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum TelemetryAttributeValueDefinition {
    #[serde(rename = "string")]
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        values: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<String>>,
    },
    #[serde(rename = "number")]
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        values: Option<Vec<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<f64>>,
    },
    #[serde(rename = "boolean")]
    Boolean {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        values: Option<Vec<bool>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<bool>>,
    },
    #[serde(rename = "string[]")]
    StringArray {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        element_values: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<Vec<String>>>,
    },
    #[serde(rename = "number[]")]
    NumberArray {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        element_values: Option<Vec<f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<Vec<f64>>>,
    },
    #[serde(rename = "boolean[]")]
    BooleanArray {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        element_values: Option<Vec<bool>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        examples: Option<Vec<Vec<bool>>>,
    },
}

impl TelemetryAttributeValueDefinition {
    pub fn attribute_type(&self) -> TelemetryAttributeType {
        match self {
            Self::String { .. } => TelemetryAttributeType::String,
            Self::Number { .. } => TelemetryAttributeType::Number,
            Self::Boolean { .. } => TelemetryAttributeType::Boolean,
            Self::StringArray { .. } => TelemetryAttributeType::StringArray,
            Self::NumberArray { .. } => TelemetryAttributeType::NumberArray,
            Self::BooleanArray { .. } => TelemetryAttributeType::BooleanArray,
        }
    }
}

/// Schema definition of one attribute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryAttributeDefinition {
    #[serde(flatten)]
    pub metadata: TelemetryAttributeMetadata,
    #[serde(flatten)]
    pub value: TelemetryAttributeValueDefinition,
}

/// Schema definition of a start or event attribute, including requiredness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryRequiredAttributeDefinition {
    #[serde(flatten)]
    pub attribute: TelemetryAttributeDefinition,
    pub required: bool,
}

pub type TelemetryStartAttributeDefinition = TelemetryRequiredAttributeDefinition;
pub type TelemetryEventAttributeDefinition = TelemetryRequiredAttributeDefinition;

/// Schema definition of one event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEventDefinition {
    pub description: String,
    pub attributes: TelemetryMap<TelemetryEventAttributeDefinition>,
}

/// Parent relationship allowed for a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TelemetryParentDefinition {
    Any,
    RootOrExternal,
    Spans { spans: Vec<String> },
}

/// Default status and error condition of a span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySpanStatusDefinition {
    pub default: TelemetryDefaultStatus,
    pub error_when: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryDefaultStatus {
    Ok,
}

/// Schema definition of one span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySpanDefinition {
    pub description: String,
    pub parents: TelemetryParentDefinition,
    pub start_attributes: TelemetryMap<TelemetryStartAttributeDefinition>,
    pub end_attributes: TelemetryMap<TelemetryAttributeDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<TelemetryMap<TelemetryEventDefinition>>,
    pub status: TelemetrySpanStatusDefinition,
}

/// Serializable telemetry schema definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetrySchemaDefinition {
    pub version: u32,
    pub spans: TelemetryMap<TelemetrySpanDefinition>,
}

/// Typed identity helper for serializable telemetry schema data.
pub const fn define_telemetry_schema(schema: TelemetrySchemaDefinition) -> TelemetrySchemaDefinition {
    schema
}
