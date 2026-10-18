# Harness telemetry traceability

Pinned source: `packages/agent/src/harness/telemetry.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`. Rust: `crates/pi-agent-core/src/harness/telemetry.rs`.

The module declares two telemetry schemas and two typed span starters on top of
the `pi-telemetry` runtime contract. The Rust port reproduces every schema
value and both starters. Differential proof compares the full serialized
schemas and recorded spans with the pinned TypeScript module.

| TypeScript source | Observable contract | Rust item and proof |
|---|---|---|
| `telemetry.ts:42-118` | `AI_TELEMETRY_SCHEMA` version 1 with one `pi.ai.request` span, six start attributes, fifteen end attributes, no events, default `ok`, exact error condition | `ai_telemetry_schema()`; `declares_the_ai_request_schema`, differential `telemetry_schemas` |
| `telemetry.ts:120-136` | AI span name/attribute/span type helpers | `AiSpanName`, `AiSpan`, `AiTelemetrySpan`, `AiSpanAttributes`, `AiSpanStartAttributes`, `AiSpanEndAttributes`; `span_names_expose_the_source_literals` |
| `telemetry.ts:138-145` | `startAiSpan(context, name, attributes, callback)` | `start_ai_span`; `records_ai_and_harness_spans_through_the_composed_context`, differential `telemetry_spans` |
| `telemetry.ts:147-181` | `HOOK_NAMES` and `EVENT_TYPES` literal sets used by the harness schema | Inlined exact value lists in the `pi.harness.hook` and `pi.harness.event_handler` start attributes; differential `telemetry_schemas` |
| `telemetry.ts:183-231` | Shared operation start/error attribute definitions | `operation_start_attributes`, `operation_error_attributes`; differential `telemetry_schemas` |
| `telemetry.ts:232-573` | `HARNESS_TELEMETRY_SCHEMA` version 1 with eleven spans, exact parents, attributes, values, cardinalities, empty events, default `ok`, exact error conditions | `harness_telemetry_schema()`; `declares_every_harness_span_in_the_source_order_and_combined_schema_order`, differential `telemetry_schemas` |
| `telemetry.ts:575` | `AGENT_TELEMETRY_SCHEMAS = [AI, HARNESS]` | `agent_telemetry_schemas()`; focused order test and differential `telemetry_schemas` |
| `telemetry.ts:577-600` | Harness span name/attribute/span type helpers | `HarnessSpanName`, `HarnessSpan`, `HarnessTelemetrySpan`, `HarnessSpanAttributes`, `HarnessSpanStartAttributes`, `HarnessSpanEndAttributes`; `span_names_expose_the_source_literals` |
| `telemetry.ts:602-614` | `startHarnessSpan(context, name, attributes, callback)` | `start_harness_span`; `records_ai_and_harness_spans_through_the_composed_context`, `maps_callback_failures_to_error_status`, differential `telemetry_spans` |

## Internal branches

| TypeScript source | Observable contract | Rust proof |
|---|---|---|
| `telemetry.ts:138-145` | Callback receives the admitted span; failure status follows the `pi-telemetry` contract | `start_ai_span` delegates to `TelemetryContextExt::start_span`; `maps_callback_failures_to_error_status` |
| `telemetry.ts:602-614` | Same admission path for harness spans | `start_harness_span`; focused and differential span tests |
| `telemetry.ts:183-231` | Operation spans share start/error attributes, then add their own kind/outcome | `operation_span` merges the shared maps; differential schema equality covers run, compaction, and navigation |
| `telemetry.ts:232-573` | Span map insertion order is `run, compaction, navigation, checkpoint, turn, step, tool, hook, sleep, event_handler, session.write` | `pi_telemetry::TelemetryMap` (insertion-ordered `IndexMap`); the focused test asserts the source span order and the differential checks key order of every schema map (`map_key_orders`) |
| `telemetry.ts:42-118` | AI span has no declared events | `events: None`; `declares_the_ai_request_schema` |
| `telemetry.ts:232-573` | Every harness span has no declared events | `events: None` for all eleven spans; differential schema equality |

## Gaps and discrepancies

- **Type-level inference helpers have no Rust equivalent**:
  `AiSpanStartAttributes<Name>`, `AiSpanEndAttributes<Name>`,
  `AiSpanEventName<Name>`, `AiSpanEventAttributes<Name, EventName>`,
  `HarnessSpanStartAttributes<Name>`, `HarnessSpanEndAttributes<Name>`,
  `HarnessSpanEventName<Name>`, `HarnessSpanEventAttributes<Name, EventName>`,
  and the `ExactTelemetryAttributes` constraint. Rust exports runtime aliases
  (`*Span`, `*TelemetrySpan`, `*Attributes`) and enum names instead, consistent
  with the `pi-telemetry` gap record.
- **Schema constants are functions.** `AI_TELEMETRY_SCHEMA`,
  `HARNESS_TELEMETRY_SCHEMA`, and `AGENT_TELEMETRY_SCHEMAS` are `LazyLock`
  backed and exposed through `ai_telemetry_schema()`,
  `harness_telemetry_schema()`, and `agent_telemetry_schemas()` because Rust
  cannot build populated maps in a `const` item.
- **Documentation generator.** `packages/agent/scripts/generate-telemetry-docs.ts`
  and the checked-in `docs/telemetry-schema.md` are build-time documentation,
  not a runtime module; they are not ported. The schema data they render is
  covered by the differential.
