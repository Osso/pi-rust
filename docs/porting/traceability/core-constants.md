# coding-agent core constant modules traceability

Pinned sources at `532f7d12cd19235081bd4d6e425bde185652d86f`:

- `packages/coding-agent/src/core/provider-display-names.ts`
- `packages/coding-agent/src/core/radius.ts`
- `packages/coding-agent/src/core/tool-capabilities.ts`
- `packages/coding-agent/src/core/diagnostics.ts`
- `packages/coding-agent/src/core/session-errors.ts`

Differential proof: request `27004` compares provider display names (ordered entries), `RADIUS_PROVIDER_ID`, both tool-name arrays, and the thrown `SessionImportFileNotFoundError` (`name`, `message`, `filePath`) against the pinned modules. Focused tests: `crates/pi-coding-agent/tests/core_constants.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `BUILT_IN_PROVIDER_DISPLAY_NAMES` | 32 provider id → name pairs | `BUILT_IN_PROVIDER_DISPLAY_NAMES: [(&str, &str); 32]` in source order; `built_in_provider_display_name` reproduces record lookup; differential `27004` |
| `RADIUS_PROVIDER_ID` | `"radius"` | `RADIUS_PROVIDER_ID`; differential `27004` |
| `CHILD_DISABLED_AGENT_TOOL_NAMES` / `SUPERVISOR_ONLY_TOOL_NAMES` | Ordered tool-name lists | `[&str; 7]` / `[&str; 2]`; differential `27004` |
| `ResourceType` / `ResourceCollision` | `extension \| skill \| prompt \| theme`; camelCase wire record with optional sources | `ResourceType`, `ResourceCollision`; `serializes_resource_diagnostics_with_typescript_wire_names` |
| `ResourceDiagnostic` | `type`, `message`, optional `path`/`collision` | `ResourceDiagnostic` with `type` rename and omitted-when-absent fields; same test |
| `SessionImportFileNotFoundError` | `name`, `message` = `File not found: <path>`, `filePath` | `SessionImportFileNotFoundError` (`NAME`, `Display`, `file_path`); differential `27004` |

## Exclusions

- `ResourceDiagnostic`/`ResourceCollision` are Rust records with typed fields instead of TypeScript interfaces; there are no runtime validators in the source, so none were invented.
- JavaScript `Error` subclassing maps to `std::error::Error` plus a `NAME` constant.

## Discrepancies

None.
