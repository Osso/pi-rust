# utils/diagnostics traceability

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Pinned source: `packages/ai/src/utils/diagnostics.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `DiagnosticErrorInfo` | `name?`, `message`, `stack?`, `code?: string \| number`; wire names unchanged. | `utils::diagnostics::DiagnosticErrorInfo` / `DiagnosticCode` with camelCase serde, omitted `None`, and integral numbers serialized without `.0`. `serializes_with_typescript_wire_names_and_omits_absent_optionals`, `deserializes_diagnostic_codes_and_optional_fields`. |
| `AssistantMessageDiagnostic` | `type`, epoch-millisecond `timestamp`, optional `error`, optional `details`. | `AssistantMessageDiagnostic` (`r#type` renamed to `type`); same serialization tests. |
| `formatThrownValue` | `Error` → `message \|\| name`; string → itself; other → JavaScript `String(value)`. | `format_thrown_value` over `ThrownValue::{Error,Text,Json}`; `formats_errors_strings_and_json_values_like_javascript_string`. JS number/array/object formatting is reproduced (`pi_js_compat::number::js_number_to_string`). |
| `extractDiagnosticError` | Error: empty `name` → absent, `message \|\| name`, preserve stack and string/number code. Non-Error: `{name:"ThrownValue", message: formatThrownValue}`. | `extract_diagnostic_error`; `extracts_error_fields_and_keeps_supported_codes`, `empty_error_name_becomes_none_and_empty_message_falls_back_to_name`, `non_error_values_are_reported_as_thrown_values`. |
| `createAssistantMessageDiagnostic` | Current `Date.now()` timestamp plus extracted error and optional details. | `create_assistant_message_diagnostic`; `creates_a_timestamped_diagnostic_with_optional_details`. |
| `appendAssistantMessageDiagnostic` | Append to `message.diagnostics`, creating the list when absent. | `append_assistant_message_diagnostic(&mut Option<Vec<_>>, _)`; `appends_diagnostics_into_an_optional_list`. |

## Exclusions

- JavaScript `undefined`, symbols, functions, and class instances are not representable in `ThrownValue`; typed core APIs exclude invalid runtime inputs.
- `format_thrown_value` returns `""` where TypeScript would return `undefined` when both `message` and `name` are empty.
- No differential oracle: `createAssistantMessageDiagnostic` reads the wall clock, and the remaining helpers are fully covered by characterization tests. Timestamp behavior is asserted as positive epoch milliseconds.
- No dependency beyond exact `serde`/`serde_json`.

## Native resource diagnostics

`ResourceDiagnostic.message/path` and every `ResourceCollision` text field retain `JsString`. `raw_resource_diagnostics` reproduces the prior serde rejection of lone-surrogate JSON, then round-trips high/low/pair values in all fields without replacement. The test imports the actual record module; source enum tags/field names are unchanged. Extension shortcut conflict construction retains native IDs/messages; encoding occurs only at actual stderr output when UI is absent. Outside constructors in resource_loader/skills and consumers in package_manager/loaders/prompt templates remain parent-owned integration work.
