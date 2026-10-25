# version-check traceability

Frozen source: `packages/coding-agent/src/utils/version-check.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (80 lines).
Rust: `crates/pi-coding-agent-base/src/utils/version_check.rs`.
Contract: [version-check](../../specs/version-check.md).

| Source symbol/branch | Native mapping | Behavioral proof |
|---|---|---|
| `comparePackageVersions`: strict trimmed semver; invalid input | `parse_semver`, `compare_semver`, `compare_package_versions` | `suite_core_2 differential_version_check::`: 676 pairs |
| `isNewerPackageVersion`: precedence or trimmed inequality | `is_newer_package_version` | Same 676 pairs; `suite_core_3 lossless_version_check::` |
| `getLatestPiRelease`: skip/offline truthiness at call time | `read_version_check_disabled` | `version_check_release`: separate native child environments, nonempty/empty flags and all public helper paths |
| Request URL, method, accept, User-Agent and timeout signal | `get_latest_pi_release` | Actual frozen release fixture records formed Fetch requests; native compares name/version/platform/arch/format/order, with runtime-only gap below |
| 2xx, trimmed version/package/note, absent/blank values | `parse_release_payload` | Actual frozen JSON cases in `version_check_release` |
| Fetch/body/JSON errors propagate; Fetch UTF-8 BOM handling | `get_latest_pi_release`, `read_body` | Source transport, body rejection, malformed/empty JSON and BOM cases; RED: native swallowed fetch TypeError |
| Configured timeout signal; no independent transport race/retry | Shared `AbortSignal::timeout`, direct `Fetch` | Timeout rejection and transport ignoring its timeout signal |
| `getLatestPiVersion`: version projection, unchanged rejection | `get_latest_pi_version` | Source version success/error/skip cases |
| `checkForNewPiVersion`: newer-only, catch all failures | `check_for_new_pi_version` | Source newer/equal/older, transport/body/JSON failure and skip cases |
| Removed old-only formatter/retry API | Deleted formatter types/helper and retry option; self-update caller uses root Error message | Removed source-absent formatter fixtures; actual release differential replaces old-only retry tests and the previous no-assertion env placeholder |

## Gaps

- Native User-Agent runtime metadata is `rust`, not Node/Bun; every other component and its order must match (parent classification 2026-10-25; [shared authority](pi-user-agent.md)).
- Adversarial wrong-typed/exotic JSON, numeric extremes, OS/runtime faults and scheduling races follow the repository parity-bar gap; no new carriers are added for them.

## Proof state

At `0824ba3e5`: actual frozen release matrix GREEN (30 cases), retained semver/newer matrix GREEN (676 pairs each), native lossless version2, shared boundary11 and branded catalog-cause1 GREEN. `cargo check -j3 --workspace --all-targets` passes without warnings; targeted warning-mode clippy and strict oracle typecheck pass, with no changed-line warnings. Manual Rust/TS changed-line readability passes (metrics executable unavailable). The standalone release target uses a child process per environment case; only its invoked child helper is ignored. Logs: `/tmp/settings-models-version-{release,semver,lossless-version,request-identity,catalog-error}-green.log`, `/tmp/settings-models-request-identity-workspace-check.log`.
