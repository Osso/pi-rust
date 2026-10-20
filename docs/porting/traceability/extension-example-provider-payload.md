# Provider payload extension example traceability

Source: pinned `packages/coding-agent/examples/extensions/provider-payload.ts`.
Native: `crates/pi-coding-agent/examples/extensions/provider_payload.rs`.
Contract: [spec](../../specs/extension-example-provider-payload.md).

| Symbol / branch | Proof |
|---|---|
| default registration → `provider_payload_extension` | Actual shipped source compiled by `tests/example_provider_payload.rs`; explicitly registered factory loaded through native extension API |
| `before_provider_request` | Request object, nested numeric keys, Unicode/control strings, exponent-form numbers, null and array payloads; pretty JSON and two trailing newlines; unchanged return payload |
| `after_provider_response` | Status 201 with z-first/a-second record, status 204 with empty record; compact JSON and two trailing newlines |
| append/create | Exact cumulative bytes with existing content and absent log file |
| absent config directory | Both handlers report runtime errors; no file or directory created |

Differential: `oracle/example-provider-payload-oracle.ts` imports the actual pinned example, registers it with `loadExtensionFromFactory`, and dispatches through the real `ExtensionRunner`. Both sides use private temporary directories; no live logs/config/DB. Error text differs by native I/O platform; compare event names and missing output rather than normalizing message text. No output-content normalization.

Native loading is the approved project deviation. JSON uses existing JS-compatible stringify helpers, including numeric key order and number formatting. Header field correction is covered separately in [AI types](ai-types.md) and [extension types](extensions-types.md). No known module behavior gaps in valid native input domains; targeted differential GREEN: 1 test covering three filesystem scenarios and five events each.
