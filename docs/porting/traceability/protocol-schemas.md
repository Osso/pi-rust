# Protocol-schema traceability

Pinned source: `packages/protocol/src/schemas.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Direct differential proof invokes the real pinned TypeBox schemas. Accepted schema values compare exact typed fields, array order, and IEEE-754 cost bits; rejected values compare schema acceptance.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `schemas.ts:5` | `IdSchema` accepts strings with source `minLength: 1` | Focused and direct TypeBox ordinary, Unicode, whitespace, combining, embedded-NUL, and empty-field fixtures |
| `schemas.ts:7-8` | Strict objects reject additional fields | Typed-constructor exclusion; future JSONL or private wire decoders must prove this boundary |
| `schemas.ts:24-33` | `ThinkingLevelSchema` accepts exactly seven literals and excludes `ultra` | Focused round-trip test and direct TypeBox fixtures cover every literal plus five invalid spellings |
| `schemas.ts:35-45` | `SessionPhaseSchema` accepts exactly five literals | Focused round-trip test and direct TypeBox fixtures cover every literal plus five invalid spellings |
| `schemas.ts:47-50` | `ModelRefSchema` declares `provider` then `id`, both using `IdSchema` | Focused preservation and Rust diagnostic-order tests; direct TypeBox fixtures cover five accepted and three rejected pairs |
| `schemas.ts:53` | `ModelCostSchema` field order is input, output, cacheRead, cacheWrite | Focused first-invalid-field test; differential object fields use source names |
| `schemas.ts:54` | `input` accepts finite `number >= 0` | Focused integer-valued/finite/invalid tests; TypeBox finite-number, fractional, negative, and NaN fixtures |
| `schemas.ts:55` | `output` accepts finite `number >= 0` | Focused fractional/finite/invalid tests; TypeBox fractional, negative, and infinity fixtures |
| `schemas.ts:56` | `cacheRead` accepts finite `number >= 0` | Focused zero/subnormal/invalid tests; TypeBox subnormal, negative, and infinity fixtures |
| `schemas.ts:57` | `cacheWrite` accepts finite `number >= 0` | Focused negative-zero/invalid tests; TypeBox exact-bit and NaN fixtures |
| TypeBox number/minimum guards | Reject NaN, infinities, and negative values; accept negative zero | 6 focused tests plus 11 direct TypeBox differential fixtures |
| TypeBox string/minLength guards | Every Rust-representable non-empty string satisfies `minLength: 1`; empty strings fail | Focused and direct TypeBox `ModelRef` fixtures; JS-only lone surrogates documented as unrepresentable |
| `schemas.ts:60-64` | `ModelMetadataSchema` starts with non-empty provider, id, name, and api strings | Focused Rust diagnostic-order tests plus direct TypeBox one-character, Unicode/whitespace/combining/NUL, and empty-field fixtures |
| `schemas.ts:65-66` | `reasoning` is boolean; `input` is an array of exact `text|image` literals with no minimum length | Typed bool/enum fields plus direct TypeBox empty, ordered duplicate, invalid-literal, and true/false fixtures; wrong runtime types excluded |
| `schemas.ts:67-68` | `contextWindow` and `maxTokens` are integer-valued numbers greater than or equal to 1 | Direct TypeBox `1`, `Number.MAX_SAFE_INTEGER`, and zero fixtures; focused Rust `u64::MAX` coverage; positive TypeBox integers above `u64::MAX` are an approved typed-core exclusion |
| `schemas.ts:69-71` | Cost uses `ModelCostSchema`; supported thinking levels are non-empty; authenticated is boolean | Direct TypeBox fractional/negative-zero/negative/nonfinite cost, all-level/order/duplicate/empty/`ultra`, and authenticated true/false fixtures |
| `schemas.ts:103-109` | Usage token counts are non-negative integers; reasoning is optional and totalTokens is preserved | Typed `u64`/`Option<u64>` fields; focused zero/`u64::MAX` proof; direct TypeBox zero/`Number.MAX_SAFE_INTEGER`, omission-versus-zero, and non-recomputation fixtures; invalid token numbers are excluded by the typed boundary |
| `schemas.ts:110-116` | Usage cost has five finite non-negative number fields | Focused fractional/negative-zero/subnormal/maximum/first-invalid proof plus direct TypeBox negative fixtures for every field and NaN/positive-infinity/negative-infinity fixtures |
| `schemas.ts:75-78` | Text content has exact `type: "text"` property and an unrestricted string payload | Distinct Rust type plus focused string tests and direct TypeBox ordinary/empty/Unicode/combining/supplementary/NUL/wrong-tag fixtures |
| `schemas.ts:79-83` | Thinking content has exact `type: "thinking"` property, unrestricted string, and optional boolean redacted field | Distinct Rust type plus focused string/`Option<bool>` tests and direct TypeBox omitted/false/true/wrong-tag fixtures |
| `schemas.ts:84-88` | Image content has exact `type: "image"` property, unrestricted string data, and non-empty MIME type | Distinct Rust type plus focused preservation/rejection tests and direct TypeBox empty/Unicode/NUL data, whitespace MIME, wrong-tag, and empty-MIME fixtures |
| `schemas.ts:233-240` | Session metadata has non-empty id, non-negative timestamps, optional non-empty parent/cwd, and optional unrestricted name | Focused omission/presence, empty-name, `u64::MAX`, and Rust diagnostic-order proof plus 13 direct TypeBox fixtures (`6000..=6012`) covering shared timestamp boundaries, exact optional omission, Unicode/NUL/whitespace, and empty id/parent/cwd rejection |
| `schemas.ts:269-277` | Protocol errors use exactly seven code literals | Focused exact enum/string round trips plus 14 direct TypeBox fixtures (`7000..=7013`) covering all literals and seven invalid spellings |
| `schemas.ts:286-313` | Nine command leaves preserve exact literals, optional create fields, non-empty IDs/cwd, unrestricted prompt text, and typed model/thinking values | Focused constructor/getter/error tests for every leaf; direct TypeBox proof pending |
| `schemas.ts:314-326` | `CommandSchema` is the union of all nine leaves and exposes their command names | Focused `Command::as_str` coverage for every variant; direct TypeBox proof pending |
