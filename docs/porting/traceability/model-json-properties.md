# Model JSON properties (Q15)

Contract: [native model JSON](../../specs/model-json-properties.md). Common pin `4f9e6f8a4`.

## Literal source observability

- `model-registry.ts` getAll returns `this.models`; getAvailable filters without reconstructing entries.
- `rpc-mode.ts:508–510` returns getAvailable directly as `{models}`. `:491–497` returns the selected model after set_model. `:468–483` includes `session.model` in get_state. `:62–64` writes `serializeJsonLine(obj)`.
- `agent-session.ts:4151–4159` assigns `agent.state.model = model`; unknown cached properties therefore remain externally visible.
- `model-resolver.ts:378` selects from getAll. Neither registry nor resolver provides an unobservable narrowing boundary.

## Current representation/proof

`Model` retains known values once in typed fields, `Model.name` as JsString, and `ModelProperties` containing only unknown values plus original property-order metadata. Custom serialization interleaves updated typed fields and unknown fields according to original order, appending newly introduced known fields. Deserialization rejects opaque unpaired IDs and null optional fields explicitly (2026-10-22 user-approved gap).

RED `model-native-order-name-red-3.log` reproduces all three: lost/interleaved properties, rejected raw display name, silently omitted headers:null. GREEN `model-native-coding-green-1.log`: three targeted tests, including literal JSON bytes of an actual frozen startup cached model with extra keys before/between/after known fields and a native unknown nested string. No JSON-order normalization.

Native display-name search candidates remain native; Bedrock name matching stays native rather than projecting through UTF-8. Other producer/name staging rows remain under the broader Q15/AI lane and are not promoted by this carrier checkpoint.

## Nested cost properties

`model-nested-cost-red.log` reproduces an actual frozen-valid cached cost object whose unknown native string failed the previous flattened serde decoder. ModelCost and ModelCostTier now own their own unknown-only properties/order metadata, so replacing a cost object discards its old extras rather than keeping a parallel parent copy. `model-nested-cost-tier-green.log` compares full literal frozen cached-model JSON with interleaved cost fields and a tier containing native/unknown keys around known rate fields. Mechanical cost/tier literals receive empty metadata; no frozen error expectation is weakened.

## Pending

Nested typed compat unknowns, reconstruction/spread ordering, complete RPC mode proof and all-target constructor migration remain pending. This is not whole-model or registry parity acceptance.
