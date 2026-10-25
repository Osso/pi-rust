# Supervisor service

Match `supervisor/service.ts` at frozen `4f9e6f8a4`.

- Prompt construction preserves the exact frozen cumulative-objective and requirement-authority text for all five request kinds, followed by the two-space JSON request fields in source order.
- Instruction review accepts only the six exact uppercase tokens. It does not trim, fold case, infer a decision, or accept structured objects. Rejection feedback and reason ordering match frozen.
- Instruction review context contains only the frozen stateless-veto system prompt and the supplied user message/timestamp. Preserve native UTF-16 instruction text; do not insert tools or historical context.
- Response parsing preserves per-kind validation, JSON/text-object handling and source response field order.
- Request processing preserves cancellation/deadline/approval-preemption ordering, awaits evaluator abort cleanup, and completes durable requests only at the frozen boundary. Unexpected evaluation rejection propagates to the caller.

Traceability: [service](../porting/traceability/supervisor-service.md).
