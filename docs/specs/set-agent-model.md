# Set agent model

Source: `packages/coding-agent/extensions/agents-core/src/agent-model-tool.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.

- Register `set_agent_model` without approval. Require string `agentId`; allow only optional string `provider`, `modelId`, and effort `off|minimal|low|medium|high|xhigh|max|ultra`; reject additional properties.
- Trim provider/model using JavaScript whitespace. Two blank/missing values request no model change. A single nonblank value fails before resolving the agent. Require at least a model pair or effort.
- Resolve the explicit agent ID through the runtime's live child sessions. Reject absent agents, background agents, inactive lifecycle, missing live sessions and sessions without mutation support, in that order. Never select/view another agent.
- Find the requested model in the live target registry. Model selection accepts optional effort in the same operation; effort-only uses the live effort setter. Clamp to actual model capabilities. Return effective target values, omitting provider/modelId if the target has no model.
- Keep the current request and its cancellation token unchanged; apply effective values to the next request. Publish effective model/effort before mutation through the existing store ownership/revision API. Rejected publication must leave live values unchanged. Child changes must not overwrite parent/global defaults.

No legacy waivers renewed. Native extensions remain Rust-native under the project extension policy. Typed core APIs exclude invalid JavaScript runtime types; JSON tool arguments remain schema-validated.

Integration: bounded tool work through `58301d0a` merged via `128df8af`. Shared-validator optional-null and union-order repairs are integrated by `8d9bb557` (through `f86144ba`). Independent model-tool verification by agent41 is pending in `/tmp/pi-rust-repin-validation-model-verify`; the historical admission RED is not a current result, and no current acceptance proof or whole agent-session parity is claimed. Proof and exact pending boundaries: [traceability](../porting/traceability/agent-model-tool.md).
