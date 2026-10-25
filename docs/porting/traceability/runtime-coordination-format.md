# Runtime coordination format

Whole-module audit: `packages/coding-agent/src/core/runtime-coordination-format.ts` (59 lines), frozen `4f9e6f8a4`; [source lock](../source-lock.json). Rust: `crates/pi-coding-agent-storage/src/core/runtime_coordination_format.rs`. No production change is needed.

| Frozen symbol / branch | Native mapping and proof |
| --- | --- |
| `SHARED_CHANNEL_RECEIVE_GUIDANCE` | Exact constant, including empty-channel leading separators; scalar differential. |
| `formatRuntimeMailboxPrompt` | `format_runtime_mailbox_prompt`: same/cross-session sender lines, unknown sender/default main agent, absent/empty/remote sender host, empty-body substitution and preservation of nonempty untrimmed bodies; scalar and raw UTF-16 differentials. |
| `formatSharedChannelPrompt` | `format_shared_channel_prompt`: input ordering, double-newline message separators and guidance suffix; empty/multi-message differential. |
| `formatSharedChannelMessage` | `shared_channel_message`: trimmed body, empty fallback and origin-host-dependent sender suppression; scalar remote/empty host cases. |
| `formatRuntimeMailboxFileReferences` | `format_file_references`: absent/null/empty lists omit the header; populated lists preserve order and append after body. Existing invalid-list/null-reference errors stay explicit. |
| `formatRuntimeMailboxFileReference` | Inline reference formatting in `format_file_references`: optional truthy label plus em dash, then path; labeled/unlabeled/empty-label scalar cases and raw UTF-16 labels/paths. |

Fresh proof pending: 14-case `suite_core_2 differential_runtime_coordination_format` and two `suite_core_3 lossless_coordination_format` cases. The oracle loads the unmodified common pin. Existing raw mailbox tests cover sender/recipient/host/body/ref units and the two already-supported invalid-reference errors.

Gap: adversarial/runtime oddities and unpaired units beyond the existing scalar shared-row carriers follow the approved parity exclusions; mailbox text and references retain their lossless carriers.
