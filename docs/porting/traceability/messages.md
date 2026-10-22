# Harness messages — common-pin re-port

Frozen `4f9e6f8a4`, `packages/agent-core/src/harness/messages.ts` (184 lines).
Rust `crates/pi-agent-core/src/harness/messages.rs`; runtime exports forwarded by harness/root.

| Frozen lines/symbol | Native representation | Proof |
|---|---|---|
| 4–17 four summary markers | exact ASCII/LF constants | `summary_markers`; native frozen builder/conversion differential compares all four values |
| 19–29 BashExecutionMessage | role enum, native command/output/path strings, unrestricted f64 exit/timestamp | existing value-record tests and native frozen numeric rendering matrix |
| 31–66 CustomMessage, BranchSummaryMessage, CompactionSummaryMessage and declaration merging | native custom AgentMessage records, raw structured details, native content union; optional numeric/provider-native fields retained | native builder + conversion differential; session native-context differential |
| 69–87 bashExecutionToText | native joins; output/no-output; cancel precedence; nonzero JS numeric exit; nonempty truncation path | existing 13 bash fixtures + native frozen 16-case number/cancel matrix (±0, fractional, negative, NaN, ±Infinity and raw UTF-16) |
| 89–97 createBranchSummaryMessage | native summary/fromId, shared V8 Date parser numeric timestamp | native frozen builder matrix over ISO/epoch/invalid/raw-surrogate date strings |
| 99–125 createCompactionSummaryMessage / options | native raw record, optional numeric fields, providerNative JSON | same matrix asserts every optional field, including fractions and native nested provider data |
| 127–142 createCustomMessage | native text/image blocks, display flag, absent vs explicit null/details JSON, Date timestamp | same matrix covers all details states and image/text content |
| 144–184 convertToLlm | native user/assistant/tool pass-through; bash/custom/branch/compaction projections; excluded bash/unknown-role drops | native raw conversion matrix + existing scalar-role and loader/messages differential |

## Domains and approvals

- Human text, arbitrary content/details JSON, native provider payload values and constructor timestamps have exact common-pin behavior; old U+FFFD/lone-surrogate exclusions are removed, not carried forward.
- JS constructor Date NaN serializes as null while remaining an f64/JsValue number in native memory; no validation or normalization was added.
- Standard typed messages and provider-native contexts exclude invalid JS runtime shapes per AGENTS. Cycles/functions/symbols/undefined as arbitrary JSON values are not native JSON inputs. Optional undefined fields map to absence; explicit null details remain present.
- Pass-through/getter values are owned, per the approved 2026-10-22 general rule. Coding consumers preserve mutations via native setters; no JS identity promise is made.
- No source discrepancies or new deviations found by whole-module comparison.

Proof: `repin_harness_messages` has three grouped direct-module tests (12 builder/date/details cases, 16 bash numeric/cancel cases, native role conversion); `/tmp/pi-rust-repin-delta-agent-core/messages-repin-green.log`. Existing `differential_harness_core` covers the full scalar conversion roles, Bash no-output/path branches and plain/image custom content. Earlier native persistence/compaction tests cover standard message pass-through.
