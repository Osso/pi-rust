# coding-agent experimental auth traceability

Pinned source: `packages/coding-agent/src/cli/experimental/auth.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27007` passes six `RawAuthOptions` shapes (neither, token, file, both, empty token, empty file) through the pinned module and the Rust port, comparing the exact `{auth?, errors}` records. Focused tests: `crates/pi-coding-agent/tests/experimental_auth.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `AuthInput` | `{type:"token",token}` or `{type:"file",path}` | `enum AuthInput` (lowercase `type` tag); serialization asserted in `parses_token_and_file_sources` |
| `RawAuthOptions` | Optional `authToken`, `authTokenFile` | `RawAuthOptions { auth_token, auth_token_file }` |
| `parseAuthInput` | Both set → exclusive error only; token → token source; file → file source; neither → empty result | `parse_auth_input`; focused cases and differential `27007` |

## Exclusions

- Presence is modelled with `Option`; `undefined` and an absent key are the same value, as in the source.
- The source returns an object with an absent `auth` key; `ParsedAuthInput { auth: None }` maps to the same wire shape.

## Discrepancies

None.
