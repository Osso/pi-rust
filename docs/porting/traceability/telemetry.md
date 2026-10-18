# Telemetry traceability

Pinned source: `packages/telemetry/src/{index,memory,noop}.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. Rust code: `crates/pi-telemetry/src/{lib,memory,noop}.rs`. Rust code/proof commit: `0b222a5673351c2b870ce21f3a8ccb687533dbfc`.

## Public exports

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `index.ts:1` `AttributeValue` | `string \| number \| boolean \| readonly string[] \| readonly number[] \| readonly boolean[]` | `AttributeValue` enum with `String`, `Number(f64)`, `Boolean`, `StringArray`, `NumberArray`, `BooleanArray`; untagged serde wire shape. `attribute_values_round_trip_through_their_wire_shape`. |
| `index.ts:3` `SpanAttributes` | String-keyed map whose values may be `undefined` | `pub type SpanAttributes = BTreeMap<String, AttributeValue>`. JavaScript `undefined` entries are not representable and are therefore omitted. Covered by recording/snapshot tests and the differential oracle. |
| `index.ts:7` `SpanOptions` | `{ name; attributes? }` | `SpanOptions { name: String, attributes: Option<SpanAttributes> }`, camelCase serde. Covered by every span test and the oracle scenarios. |
| `index.ts:12` `SpanStatus` | `{ status: "ok" } \| { status: "error"; error?: { name; message } }` | `SpanStatus::{Ok, Error { error: Option<SpanError> }}` with `#[serde(tag = "status")]`; `SpanError { name, message }`. Covered by status tests and oracle `status`/`settled` scenarios. |
| `index.ts:14` `TelemetryContext` | `startSpan<T>(options, callback): Promise<T>` | `TelemetryContext::start_span_erased` (dyn-safe backend) plus `TelemetryContextExt::start_span` (typed `Result<T, E>`). `&T` and `Arc<T>` delegate. Covered by noop, memory, and typed-starter tests. |
| `index.ts:18` `TelemetrySpan` | Span is also a context; `addEvent`, `setAttributes`, `setStatus` | `TelemetrySpan` concrete struct over private `SpanInner::{Noop, InMemory}`; implements `TelemetryContext`; methods `add_event`, `set_attributes`, `set_status`. Covered by nested/parentage, settled-inert, and noop tests. |
| `index.ts:24` `NOOP_TELEMETRY_CONTEXT` | Shared inert context | `noop::NOOP_TELEMETRY_CONTEXT: NoopTelemetryContext`; `NoopTelemetryContext` also exported because Rust constants require a nameable type. Covered by noop tests. |
| `index.ts:26` `TelemetryAttributeType` | Six literal type names | `TelemetryAttributeType` enum with serde names `string`, `number`, `boolean`, `string[]`, `number[]`, `boolean[]`; `TelemetryAttributeValueDefinition::attribute_type`. Schema round-trip test. |
| `index.ts:28` `TelemetryAttributeMetadata` | `description`, optional `sensitive`, optional `cardinality` | `TelemetryAttributeMetadata` plus `TelemetryAttributeCardinality::{Low, High}`. Schema serialization test and differential `schema` scenario. |
| `index.ts:34` `TelemetryAttributeDefinition` | Metadata intersected with a type-specific value constraint | `TelemetryAttributeDefinition { #[serde(flatten)] metadata, #[serde(flatten)] value }`; `TelemetryAttributeValueDefinition` internally tagged by `type` with `values`/`examples` or `elementValues`/`examples`. Schema round-trip and differential `schema`. |
| `index.ts:44-45` `TelemetryStartAttributeDefinition`, `TelemetryEventAttributeDefinition` | Definition plus `required: boolean` | `TelemetryRequiredAttributeDefinition` with both TypeScript names exported as aliases. Schema round-trip and differential `schema`. |
| `index.ts:47` `TelemetryEventDefinition` | `{ description; attributes }` | `TelemetryEventDefinition`. Schema round-trip and differential `schema`. |
| `index.ts:52` `TelemetryParentDefinition` | `{kind:"any"} \| {kind:"root_or_external"} \| {kind:"spans"; spans}` | `TelemetryParentDefinition` internally tagged by `kind`, snake_case variants. Schema round-trip and differential `schema`. |
| `index.ts:57` `TelemetrySpanDefinition` | description, parents, start/end attributes, optional events, status | `TelemetrySpanDefinition` with `events: Option<...>` omitted when absent. Schema round-trip and differential `schema`. |
| `index.ts:66` `TelemetrySchemaDefinition` | `{ version; spans }` | `TelemetrySchemaDefinition { version: u32, spans: BTreeMap<...> }`. Schema round-trip and differential `schema`. |
| `index.ts:72` `defineTelemetrySchema` | Identity helper preserving the value | `pub const fn define_telemetry_schema`. `preserves_serializable_schema_definitions`. |
| `index.ts:318` `TypedSpanStarter` | Overload set bound to a parent context and combined schemas | `TypedSpanStarter<C>` struct; `start_span(name, attributes, callback)` passes the admitted span and a child starter bound to that span. `typed_span_starter_binds_child_starters_to_their_parent_spans`. |
| `index.ts:324-355` `bindTypedSpanStarter`, `createTypedSpanStarter` | Bind a context; schema values are never inspected | `TypedSpanStarter::new` and `create_typed_span_starter(context, &[schemas])`; the schema slice is accepted and ignored, matching the source. Typed-starter test and differential parentage. |
| `memory.ts:11` `RecordedTelemetryEvent` | `{ name; attributes }` | `memory::RecordedTelemetryEvent`, camelCase serde. Recording and snapshot tests; oracle `recording`. |
| `memory.ts:16` `RecordedTelemetrySpan` | id, parentId, name, attributes, events, status, settled, optional endSequence | `memory::RecordedTelemetrySpan` with `parent_id: Option<u64>` serialized as `null` and `end_sequence` omitted when absent. All memory tests and every oracle span scenario. |
| `memory.ts:192` `InMemoryTelemetryContext` | Isolated recorder with `startSpan` and `getSpans` | `memory::InMemoryTelemetryContext` (`Clone` shares state, matching JavaScript object identity), ids starting at 1, detached snapshots in start order. All memory tests. |
| `noop.ts:3` `startNoopSpan` | Admits callback synchronously with the shared inert span | `NoopTelemetryContext::start_span_erased` invokes the callback eagerly and returns its future. Noop tests. |
| `noop.ts:11` `noopTelemetrySpan` | Frozen span whose mutations do nothing | `TelemetrySpan::noop()` variant; `add_event`/`set_attributes`/`set_status` are no-ops. Noop tests. |

## Internal branches

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `memory.ts:50` `copyAttributeValue` | Arrays are copied | Rust `Clone` of `Vec` values; snapshot test mutates a returned array and re-reads. |
| `memory.ts:54,63` `copyAttributes`, `mergeAttributes` | Skip `undefined`; later values overwrite; do not mutate current | Rust maps cannot contain `undefined`; `create_span` clones option attributes, `set_attributes` uses `BTreeMap::extend` for overwrite-in-place merge. Recording test and oracle `recording`. |
| `memory.ts:71` `copyStatus` | Status and error details are detached | Rust `Clone` of `SpanStatus`/`SpanError`; snapshot test. |
| `memory.ts:78` `automaticErrorStatus` | Error status on throw/rejection; inspect `Error.name`/`message` when available | Typed `start_span` maps `Result::Err` to `SpanStatus::Error` with `name` = Rust error type name and `message` = `Display`; explicit status is never overwritten. `memory_preserves_error_values_and_records_automatic_error_status`, `memory_uses_last_explicit_status_without_automatic_overwrite`. |
| `memory.ts:89` `settleSpan` | First settlement wins; sequence assigned from a shared counter | `settle_span` checks `settled`, applies automatic error, then assigns `next_end_sequence`. Settled-inert, concurrency, and snapshot tests. |
| `memory.ts:101` `createSpan` | Parent id from the parent span; fresh id; initial ok status | `create_span` allocates ids under the state lock and links the parent. Parentage test and oracle `parentage`. |
| `memory.ts:120` settled parent | Child is admitted through the noop context and not recorded | `start_in_memory_span` returns the noop callback path when the parent is settled. Settled-inert test and oracle `settled`. |
| `memory.ts:120` callback throw/reject | Synchronous throw and async rejection settle the span and rethrow the same value | Typed callback returns `Result`; `Err` is returned unchanged after settlement. Error-preservation tests. |
| `memory.ts:141-179` settled span mutations | `addEvent`, `setAttributes`, `setStatus` become inert | `InMemorySpanHandle` guards each mutation on `settled`. Settled-inert test and oracle `settled`. |
| `memory.ts:200` `getSpans` | Detached snapshots in start order | `snapshot` clones attributes, events, status, and sequence. Snapshot test. |
| `index.ts:324` callback receives child starter | Child starter is bound to the admitted span | `TypedSpanStarter::start_span` constructs `TypedSpanStarter::new(span.clone())`. Typed-starter test and oracle `parentage`. |

## Explicit gaps

- **Type-level inference helpers** (`AttributeDefinitionValue`, `RequiredAttributeNames`, `OptionalAttributeNames`, `InferRequiredAndOptionalAttributes`, `InferStartAttributes`, `InferOptionalAttributes`, `ExactTelemetryAttributes`, `InferEventAttributes`, `TelemetrySchemaSpanName`, `TelemetrySchemaSpanStartAttributes`, `TelemetrySchemaSpanEndAttributes`, `TelemetrySchemaSpanEventName`, `TelemetrySchemaSpanEventAttributes`, `SchemaTelemetrySpan`, `TelemetrySchemaSpanUnion`, `TelemetrySchemaTuple`, `SpanNameInSchema(s)`, `SpanStartAttributesInSchema`, `SpanInSchema`, `DuplicateTelemetrySpanNames`, `UniqueTelemetrySchemas`, `UnionToIntersection`, `TypedSpanStarterForName`): TypeScript conditional, mapped, template-literal, and variadic-tuple types have no Rust equivalent. Rust ports the runtime schema data and starter behavior; it does not claim compile-time name/attribute correlation or duplicate-name rejection.
- **Passive failure branches** (`memory.ts:54-120` proxy `get`/`ownKeys`/`getOwnPropertyDescriptor` throws, `createSpan` allocation fallback): Rust's typed `SpanAttributes`/`SpanOptions` cannot carry unreadable JavaScript proxies, and Rust allocation failure aborts. The observable Rust contract is the non-failure path; no fallback is added.
- **`Object.freeze(noopTelemetrySpan)`** (`noop.ts:18`): Rust has no mutable noop span object to freeze; the unit variant is inert.
- **`src/testing/{index,types,conformance}.ts`** (package subpath `./testing`): not part of the tracked three-file coverage slice. Its nine conformance cases are ported as behavioral Rust tests in `crates/pi-telemetry/tests/telemetry.rs`; the runner-independent fixture factory API is not ported.
- **Cancellation**: these three files contain no `AbortSignal`; no cancellation behavior is claimed.

## Rust boundary

- JavaScript `number` maps to `f64`; no integer narrowing or validation is added. Recorded ids and sequence numbers are `u64` because the source only produces positive integers.
- JavaScript optional fields map to `Option`; `undefined` attribute entries are not representable and are omitted rather than stored.
- TypeScript accepts sync or async callback results; Rust requires `Future<Output = Result<T, E>>`, with `E: Error + Send + 'static`. Exceptions/rejections map to `Err`; a returned error value in TypeScript corresponds to `Ok` with a status set explicitly, matching the source's throw-only automatic error rule.
- Typed `start_span` erases the result through `Box<dyn Any + Send>` only to keep `TelemetryContext` dyn-compatible; the typed wrapper downcasts the exact `Result<T, E>` it constructed.

## Discrepancies

- No source bugs identified in the three files.
- Source behavior intentionally not preserved as a fallback: the `createSpan` allocation-failure path and the JavaScript proxy passivity catches. Both require runtime failure modes that the typed Rust API excludes; see gaps above.

## Proof status

- Focused RED: `cargo test -p pi-telemetry --test telemetry` exit 101 before production code, with unresolved imports for the telemetry API.
- Focused GREEN: `/tmp/pi-rust-telemetry-test.out`, exit 0; 12 behavioral tests plus 1 differential test pass.
- Differential: `/tmp/pi-rust-telemetry-test.out` runs `crates/pi-telemetry/tests/differential_telemetry.rs` against `oracle/telemetry-oracle.ts` over five scenarios: `recording`, `status`, `parentage`, `settled`, and `schema`.
- Gates: `/tmp/pi-rust-telemetry-fmt-check.out` exit 0; `/tmp/pi-rust-telemetry-clippy.out` exit 0 (`cargo clippy -p pi-telemetry --all-targets -- -D warnings`).
