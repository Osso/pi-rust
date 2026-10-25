# coding-agent messages

Frozen `4f9e6f8a4`, `packages/coding-agent/src/core/messages.ts` (239 lines). Native `crates/pi-coding-agent-data/src/core/messages.rs`. Shared constants/builders/bash formatting are re-exported from [harness messages](messages.md); coding-agent adds processed-image omission.

| Frozen symbols/branches | Native/proof |
|---|---|
| summary prefixes/suffixes, BashExecutionMessage, CustomMessage, BranchSummaryMessage | harness native records/constants; common-pin harness builder/format proofs |
| CompactionSummaryMessage/Options, createCompactionSummaryMessage, createBranchSummaryMessage, createCustomMessage | all option fields/date timestamps preserved by shared constructors; common-pin harness builder proofs |
| bashExecutionToText | output/no-output/cancel/exit/truncation branches shared with harness |
| convertToLlm role conversion | 5 grouped frozen coding-agent conversion cases cover user/assistant/toolResult/custom/bash/branch/compaction, excluded bash, providerNative |
| omitProcessedImages/omitMessageImages | image-only/mixed user/tool/custom content; before/after successful assistant; no assistant, error-only, aborted-only; same frozen differential plus 2 native image tests |
| native is_cut_point_message | user/assistant only; native boundary test (not an export of this frozen file) |

GREEN at 2683e5cbc: 4 native tests (`/tmp/ext-api-suite_core_4-messages.log`). Current added 5-case differential GREEN (`/tmp/ext-api-messages-frozen2.log`). Observations compare complete parsed provider values through native JsValue; the oracle imports the actual coding-agent module. The malformed providerNative fixture was corrected to the frozen format/value contract before proof; no implementation change or relaxed assertion.

Gap: owned-return identity and typed invalid-JS inputs follow repository approval; pathological timestamps/runtime values follow the realistic-input parity bar. Invalid dates retain native NaN through existing date carriers, not the retired zero coercion described by the old trace.
