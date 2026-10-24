# coding-agent core constant modules traceability

Current sources at4f9e6f8a4 (historical proofs below need separate revalidation):

- `packages/coding-agent/src/core/provider-display-names.ts`
- `packages/coding-agent/src/core/tool-capabilities.ts`
- `packages/coding-agent/src/core/diagnostics.ts`
- `packages/coding-agent/src/core/session-errors.ts`

Differential proof: request `27004` compares provider display names (ordered entries), both tool-name arrays, and the thrown `SessionImportFileNotFoundError` (`name`, `message`, `filePath`) against the pinned modules. Focused tests: `crates/pi-coding-agent/tests/core_constants.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `BUILT_IN_PROVIDER_DISPLAY_NAMES` | 32 provider id → name pairs | `BUILT_IN_PROVIDER_DISPLAY_NAMES: [(&str, &str); 32]` in source order; `built_in_provider_display_name` reproduces record lookup; differential `27004` |
| `CHILD_DISABLED_AGENT_TOOL_NAMES` / `SUPERVISOR_ONLY_TOOL_NAMES` | Ordered tool-name lists | `[&str; 7]` / `[&str; 2]`; differential `27004` |
| `ResourceType` / `ResourceCollision` | `extension \| skill \| prompt \| theme`; camelCase wire record with optional sources | `ResourceType`, `ResourceCollision`; `serializes_resource_diagnostics_with_typescript_wire_names` |
| `ResourceDiagnostic` | `type`, `message`, optional `path`/`collision` | `ResourceDiagnostic` with `type` rename and omitted-when-absent fields; same test |
| `SessionImportFileNotFoundError` | `name`, `message` = `File not found: <path>`, `filePath` | `SessionImportFileNotFoundError` (`NAME`, `Display`, `file_path`); differential `27004` |

## Frozen 4f9e6f8a4 capability refresh

`tool-capabilities.ts:1-11` now contains nine child-disabled tools, including `answer_question` and `set_agent_model`. `core_constants::exposes_tool_capability_constants` reproduces the missing `answer_question` entry and checks the complete ordered list. Other historical proofs above are not renewed by this bounded fix.

## Old-only Radius removal

No core/radius.ts exists in frozen. Removed native constant/module/export and only its stale core_constants/core-leaves assertions and oracle branch; retained real tool/provider/session-error comparisons. [Outside-consumer and retired-test ledger](radius-config.md). Core constants4 GREEN after deletion.

## Exclusions

- `ResourceDiagnostic`/`ResourceCollision` are Rust records with typed fields instead of TypeScript interfaces; there are no runtime validators in the source, so none were invented.
- JavaScript `Error` subclassing maps to `std::error::Error` plus a `NAME` constant.

## Discrepancies

None.
