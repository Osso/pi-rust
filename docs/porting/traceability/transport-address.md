# coding-agent transport-address traceability

Pinned source: `packages/coding-agent/src/cli/experimental/transport-address.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27009` runs 25 values (valid paths, percent escapes, dot segments, authorities, credentials, ports, queries, fragments, NUL, malformed escapes, case changes, other schemes) through `--listen` and `--connect` in the pinned module and the Rust port. Focused tests: `crates/pi-coding-agent/tests/transport_address.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `UnixTransportAddress` | `{transport:"unix",path}` | `TransportAddress { transport: TransportKind, path }`; serialization asserted in `accepts_absolute_unix_paths` |
| `TransportAddress` | Unix-only union | `TransportAddress` (no alternate variant) |
| `parseTransportAddress` URL parse failure | `Invalid <option> address "<value>"` | `parse_transport_address` via `url::Url::parse`; `rejects_unparseable_addresses` |
| Non-`unix:` protocol | `Unsupported <option> transport "<protocol>:"` | `rejects_unsupported_transports`, differential `27009` |
| Authority, port, username, password | `Unix transport address must not include an authority` | `rejects_authorities_credentials_and_ports` |
| Canonical-form check | Rejects non-`unix:///` forms, `unix:////`, `?`, `#`, and any value the URL parser would reserialize differently | `url.as_str() != value` comparison; `rejects_query_fragments_and_non_canonical_forms` |
| `decodeURIComponent(pathname)` | Decodes UTF-8 escapes; malformed escapes or invalid UTF-8 are invalid addresses | Reuses `utils::git::decode_uri_component`; `decodes_percent_escapes_in_the_path`, `rejects_nul_and_malformed_percent_escapes` |
| NUL check | Rejects decoded paths containing `\0` | `rejects_nul_and_malformed_percent_escapes` |
| `posix.isAbsolute(path)` | `Unix transport address requires an absolute path` | Checked in `parse_transport_address`; unreachable through this grammar because WHATWG pathnames always start with `/` (see Discrepancies) |

## Exclusions

- The option argument is a `&str` rather than the `"--listen" | "--connect"` union; callers pass one of the two names, which is what the messages embed.

## Discrepancies

- The source's `posix.isAbsolute(path)` branch cannot fire: every value that passes the canonical-form check has a pathname starting with `/`, and percent-encoded dot segments are removed by URL normalization (rejected earlier by the href comparison). Ported for parity; `rejects_percent_encoded_dot_segments` documents the reachable outcome.
- `url` crate parsing replaces the JavaScript WHATWG parser; differential `27009` covers the observable cases.

## Proof inventory

- Focused cases: 8.
- Differential request: `27009`, 25 cases.
