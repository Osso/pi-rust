# Compaction utilities — common-pin re-port

Frozen `4f9e6f8a4`, `packages/agent-core/src/harness/compaction/utils.ts` (163 lines).
Rust: `crates/pi-agent-core/src/harness/compaction/utils.rs`.

| Frozen lines/export | Native representation | Behavioral proof |
|---|---|---|
| 5–22 FileOperations/createFileOps | three insertion-ordered `IndexSet<JsString>` values | `compaction_utils::creates_empty_file_operations`; native frozen extraction/set-order differential |
| 25–52 extractFileOpsFromMessage | assistant tool-call projection; nonempty native string path only; read/write/edit dispatch | `repin_compaction_utils::frozen_file_operation_extraction_ignores_empty_paths_and_keeps_native_set_order`: RED exposed empty path incorrectly recorded in all three sets; exact frozen GREEN after restoring truthiness condition. Existing extraction tests cover missing/nonstring paths, nonassistant messages, unknown names and multiple calls |
| 55–61 computeFileLists | edited∪written; read-only exclusion; UTF-16 default sort | native raw-surrogate differential; existing populated/empty/default-sort tests + `differential_compaction::compute_file_lists` |
| 64–75 formatFileOperations | native joins and exact summary tags | `file_operations`, native frozen extraction/format differential; existing empty/read-only/modified/both branches |
| 79–85 safeJsonStringify | native JS JSON serializer with numeric-key enumeration | raw argument keys/values, arrays/null/nesting, numeric key ordering in native frozen serialize differential |
| 88–120 truncation/full-output helpers | 2000 UTF-16 units; preserve unpaired units and marker predicates | `repin_compaction_utils::frozen_conversation_serialization_preserves_utf16_arguments_and_full_output_branches`: short content with/without truncation metadata, already-present path, empty path, exact 2000/2001 limits, raw-surrogate split and details/null |
| 123–163 serializeConversation | native user/text/image, assistant text/thinking/tool, tool-result dispatch and exact separators | native frozen serializer differential + existing `compaction_utils` and `differential_compaction::serialize_conversation` |

## Domain and discrepancies

- Whole module compared against common pin, not the retired `532f7d12` source.
- Human strings, file paths, arbitrary JSON keys/values and truncation are exact UTF-16; no replacement projection or lossy shim. JS numeric values use the shared JS serializer.
- Cyclic/functions/symbols/undefined argument values are outside the typed native JSON boundary. Thus the private serializer's undefined/unserializable branches cannot be reached by a valid native argument JSON value; no fallback was added.
- Source `isRecord` also accepts arrays, but JSON arrays cannot own named fullOutputPath/truncation properties; those non-JSON custom properties are outside the same boundary.
- No source bugs or new semantic deviations found. Empty-path recording was a native bug, not a source behavior to preserve.

Proof artifacts: `/tmp/pi-rust-repin-delta-agent-core/compaction-utils-repin-{red,green}.log`.
