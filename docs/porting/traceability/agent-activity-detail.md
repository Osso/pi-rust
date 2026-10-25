# Agent activity detail

Whole-module audit: frozen `packages/coding-agent/src/core/agent-activity-detail.ts` at `4f9e6f8a4`. Rust: `pi-coding-agent-storage/src/core/agent_activity_detail.rs`. [Contract](../../specs/agent-activity-detail.md).

| Frozen symbol/branch | Rust / proof |
| --- | --- |
| `DETAIL_ARGUMENT_KEYS` | Same ordered ten keys; differential covers every key and competing keys. |
| `MAX_ACTIVITY_DETAIL_LENGTH` | 160 UTF-16 units. |
| `summarizeToolArguments`: absent/non-object/no string | `summarize_tool_arguments`; null, boolean, scalar, array, empty object and non-string differential cases. |
| Collapse `/\s+/g`, trim, skip empty | Shared ECMAScript whitespace predicate; whitespace matrix plus U+0085/U+200B non-whitespace. |
| First usable value | Priority/empty preceding values matrix. |
| Length ≤160 / truncation | 160/161, astral pair split, raw UTF-16 preservation differential. |

Proof: `suite_core_2::differential_core_leaves::activity_detail_matches_frozen_for_priorities_whitespace_and_utf16_cut` (pending execution at commit). New-export RED: `/tmp/runtime-services-red-exports.log`. Frozen oracle: `coding-agent-core-leaves-oracle.ts`, operation `activityDetail`.

Gap: inherited properties/exotic wrong-typed objects are outside realistic inputs. Agent-session publication integration is owned by the agent-session lane; this row covers the whole helper module only.
