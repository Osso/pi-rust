# Enumerable AgentTool metadata in AI contexts

Frozen AgentTool values structurally extend AI Tool. Their enumerable label and optional executionMode survive whole-object JSON serialization and affect token estimates. Source ownership and consumer audit: [AI types traceability](../porting/traceability/ai-types.md#enumerable-agenttool-metadata).

## What it must do

- [ ] Retain optional native UTF-16 label/executionMode strings without interpreting them as provider parameters.
- [ ] Serialize present fields in frozen built-in AgentTool order: name, label, description, parameters, executionMode; omit absent metadata and function-valued hooks.
- [ ] Match frozen Faux input usage and estimateContextTokens for labeled tools, optional execution modes and unpaired label units.
- [ ] Keep metadata out of provider adapters that explicitly map tool fields; preserve it when whole contexts are serialized.

## How it works

- [Consumer/representation audit](../porting/traceability/ai-types.md#enumerable-agenttool-metadata).

## Implementation inventory

- `crates/pi-ai/src/types.rs`: optional native metadata carriers and constructor absence.
- Existing whole-Tool serializers in Faux, estimate and proxy consume the retained fields without alternate paths.

## Tests asserting this spec

- `crates/pi-ai/tests/tool_metadata.rs`: actual frozen whole-tool wire, Faux53-vs48 harness regression, estimator and explicit provider mapping.
- `crates/pi-agent-core/tests/proxy.rs`: whole-context request serialization; AgentTool population is independently owned.

## Known gaps (current cycle)

- [ ] AgentTool-to-AI-Tool population belongs to the agent-core harness slice; carrier availability alone does not establish that integration.

## Out of scope

Arbitrary undeclared tool properties, function serialization, execution scheduling and unrelated schema/description representation changes. The audit found only label/executionMode as serializable declared AgentTool extras; no generic flattened metadata bag is added.
