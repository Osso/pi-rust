use crate::{BoxFuture, ErasedSpanCallback, SpanOptions, SpanOutcome, TelemetryContext, TelemetrySpan};

/// Inert telemetry context used when an application does not provide one.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTelemetryContext;

impl TelemetryContext for NoopTelemetryContext {
    fn start_span_erased<'a>(
        &'a self,
        _options: SpanOptions,
        callback: ErasedSpanCallback<'a>,
    ) -> BoxFuture<'a, SpanOutcome> {
        callback(TelemetrySpan::noop())
    }
}

/// Shared telemetry context used when an application does not provide one.
pub const NOOP_TELEMETRY_CONTEXT: NoopTelemetryContext = NoopTelemetryContext;
