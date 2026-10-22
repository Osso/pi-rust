# version-check traceability

Pinned source: `packages/coding-agent/src/utils/version-check.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/utils/version_check.rs`.

Differential proof: `oracle/version-check-oracle.ts` drives the pinned `comparePackageVersions`,
`isNewerPackageVersion`, and `formatVersionCheckError`; the Rust side repeats every request
(`crates/pi-coding-agent/tests/differential_version_check.rs`): 26 × 26 comparison/newer pairs and
the error-formatting fixtures. The network path is proven behaviorally with an injected `Fetch`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `version-check.ts:5-6` | Latest-version URL and 10 s default timeout | `LATEST_VERSION_URL`, `DEFAULT_VERSION_CHECK_TIMEOUT_MS` |
| `version-check.ts:8-13` | Release shape (`version`, optional `packageName`, optional `note`) | `LatestPiRelease`; `returns_package_metadata_and_notes` |
| `version-check.ts:15-32` | Error formatting: deduplicated errno codes, else first non-empty cause message, else the root message | `format_version_check_error`; differential fixtures + `version_check_error_formatting_matches` |
| `version-check.ts:34-41` | Strict semver comparison through `node-semver`; `undefined` for invalid input | `parse_semver`, `compare_semver`, `compare_package_versions`; 676 differential pairs |
| `version-check.ts:43-49` | Newer check: invalid candidate falls back to trimmed string inequality | `is_newer_package_version`; 676 differential pairs |
| `version-check.ts:51-88` | `getLatestPiRelease`: offline short-circuit, one request with user agent and JSON accept header, optional retry, 2xx only, trimmed non-empty `version` | `get_latest_pi_release`; `uses_the_version_check_api_with_a_pi_user_agent`, `retries_only_when_requested`, `timeouts_are_configurable`, `offline_and_skip_environment_flags_are_honored_in_child_processes` |
| `version-check.ts:90-95` | `getLatestPiVersion` returns only the version string | `get_latest_pi_version`; `returns_only_newer_versions` |
| `version-check.ts:97-108` | `checkForNewPiVersion`: `PI_SKIP_VERSION_CHECK` short-circuit, newer-only result | `check_for_new_pi_version`; `returns_only_newer_versions`, offline/skip child probe |

## Gaps

- `getLatestPiRelease` and `checkForNewPiVersion` read `PI_OFFLINE` / `PI_SKIP_VERSION_CHECK` at call
  time, as the source does. The flag tests run in child processes so the variables cannot leak.
- `formatVersionCheckError` takes a typed `VersionCheckError`; the TypeScript `unknown` input and its
  property probing are excluded by the typed-core rule. Callers construct the same `message`/`causes`
  shape the source reads.

## Discrepancies

- None observed. Strict parsing, precedence (including prerelease/build rules), and error strings
  match `node-semver` through the pinned module.

## Proof inventory

- Differential: 676 comparison requests + 676 newer requests + error-formatting fixtures.
- Behavioral cases: 6.

## Q18 native version/release continuation (2026-10-22)

Version inputs, release version/packageName/note, and nested diagnostic text use
JsString. Release response parses through JsValue; network body bytes decode only
at ingress. User-agent text encodes only at HttpRequest header construction.

Strict semver parsing checks UTF-8 only at its ASCII grammar: unmatched units
mean invalid semver. `is_newer_package_version` then performs the source's native
trimmed-string inequality fallback; invalid strings are not silently equated.
Parsed Semver numeric/prerelease/build internals remain validated ASCII data.

Raw semver fallback, raw error text, raw user-agent version, and escaped raw release
fields have fixtures; written but unexecuted while coding compilation is blocked.
replacement-version-1/replacement-adapters-1 show zero version/user-agent primaries.
No runtime PASS or renewed frozen differential claim.
