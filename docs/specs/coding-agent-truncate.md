# Coding-agent truncation

Port `packages/coding-agent/src/core/tools/truncate.ts` at the common pin. Distinct from [agent-core truncation](truncation.md).

## Contract

- Preserve UTF-16 text until frozen Buffer encoding/decoding boundaries.
- Keep full metadata, LF counting, whole-line selection and source limit precedence.
- Accept JavaScript binary64 limits, including negative, fractional, signed-zero and nonfinite values. Omitted line limits use the frozen default.
- Tail byte slicing subtracts the raw number. Fractional Buffer property lookups are undefined; only Buffer.slice coerces the offset. Preserve the resulting source discrepancy: a fractional byte limit can return replacement characters or exceed the stated byte limit.
- Single-line truncation compares UTF-16 length, applies JavaScript slice normalization and appends `... [truncated]`. Negative and NaN limits are not clamped before comparison/slicing.
- Invalid JavaScript runtime input types remain outside typed native APIs.

## Proof

[Traceability](../porting/traceability/coding-agent-truncate.md). Direct frozen head/tail/line full-metadata corpus and explicit numeric cases: `differential_coding_agent_truncate`; UTF-16 boundary: `lossless_session::tool_truncation_keeps_units_until_the_source_utf8_suffix_boundary`. Constants and size formatting have separate linked proofs.
