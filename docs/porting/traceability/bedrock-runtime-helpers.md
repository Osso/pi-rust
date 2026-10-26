# Bedrock runtime helpers (Rust-only)

These modules replace AWS SDK machinery used by
`ai/src/api/bedrock-converse-stream.ts`. They have no TypeScript source module;
parity is proven through the ConverseStream differential oracle.

## `bedrock_sigv4.rs`

- `sign_request`: SigV4 `AWS4-HMAC-SHA256` for `bedrock` (canonical request,
  double-escaped canonical path, sorted signed headers, payload hash, session
  token, lowercase hex).
- `AwsCredentials`, `SigningError`, `format_amz_date`, `aws_escape_uri`.
- Tests: differential signer check re-signs every recorded non-bearer oracle
  request with the fixed clock and compares `Authorization` exactly; behavioral
  tests exercise bearer/skip-auth alternatives.

## `bedrock_eventstream.rs`

- `EventStreamDecoder`: incremental `application/vnd.amazon.eventstream`
  framing with prelude CRC32 and message CRC32 checks, header decoding, and the
  SDK's checksum error messages.
- `EventStreamMessage`, `EventStreamError`, `crc32`.
- Tests: `reports_crc_checksum_failures`; differential `corrupt-message-crc`,
  `corrupt-prelude-crc`; all event scenarios exercise split-frame decoding.

## `bedrock_credentials.rs`

- `env_credentials`, `profile_name`, `profile_credentials`, `profile_region`.
- Shared credentials/config INI parsing (credentials file overrides static keys,
  config file supplies profile regions and `sso-session` blocks), SSO token
  cache (`~/.aws/sso/cache/<sha1(key)>.json`) and `GetRoleCredentials` through
  the shared `Fetch` contract.
- Tests: `reads_static_profile_credentials`; differential `profile-credentials`.
- Gaps: SSO exchange has no test; SSO-OIDC refresh, `credential_process`, web
  identity, role assumption, ECS, and IMDS are not implemented (see
  `bedrock-converse-stream.md`).

## `utils/base64.rs`

- `decode` matches `atob` leniency (whitespace ignored, missing padding
  accepted, invalid/extra padding rejected); `encode` is canonical padded
  base64. Used to canonicalize image bytes before Bedrock serialization.
- Tests: exercised through `rich-messages` differential (image content) and the
  message-conversion behavioral test.

## UTF-8 file decoding (2026-10-26)

| Frozen SDK source | Contract | Proof |
|---|---|---|
| `@smithy/core/dist-cjs/submodules/config/index.js:183,270` (`readFile(..., "utf8")`) | Invalid credential/cache/INI bytes become U+FFFD; other data survives; genuine I/O errors unchanged | `bedrock_credentials::utf8_tests::{shared_ini_replaces_invalid_utf8,sso_token_replaces_invalid_utf8}`; RED `/tmp/utf8-ai-red.log`, GREEN `/tmp/utf8-ai-green.log` (offline, no provider request) |
