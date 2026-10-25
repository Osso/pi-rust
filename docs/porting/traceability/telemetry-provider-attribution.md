# telemetry (coding-agent)

Source: `packages/coding-agent/src/core/telemetry.ts` (13 lines).
Rust: `crates/pi-coding-agent-core/src/core/telemetry.rs`.
Tests: `crates/pi-coding-agent/tests/telemetry_provider_attribution.rs`.

| TS export / branch | Rust item | Test |
|---|---|---|
| `isTruthyEnvFlag` (`1`/`true`/`yes`, case-insensitive, empty false) | `is_truthy_env_flag` | `telemetry_env_flags_match_the_source` |
| env present → flag; env absent → `settings.getEnableInstallTelemetry()` | `is_install_telemetry_enabled` | `telemetry_falls_back_to_settings_when_env_is_absent`, `telemetry_env_flags_*` |

Gaps: `None` means "read `PI_TELEMETRY`", matching the TypeScript default
parameter; callers cannot force the "env unset" branch while the process has
`PI_TELEMETRY` set.

# provider-attribution

Source: `packages/coding-agent/src/core/provider-attribution.ts` (97 lines).
Rust: `crates/pi-coding-agent-core/src/core/provider_attribution.rs`.
Tests: `crates/pi-coding-agent/tests/telemetry_provider_attribution.rs` (ports
the pure header cases of `test/sdk-openrouter-attribution.test.ts`).

| TS branch | Rust item | Test |
|---|---|---|
| `matchesHost` | `matches_host` | NVIDIA/Cloudflare/OpenCode host cases |
| OpenRouter provider or `openrouter.ai` substring | `is_openrouter_model` | `adds_default_attribution_headers_for_openrouter_models`, `legacy_substring_matching_is_preserved` |
| NVIDIA provider or exact host | `is_nvidia_nim_model` | `nvidia_provider_and_host_get_billing_header` |
| Cloudflare providers or exact hosts | `is_cloudflare_model` | `cloudflare_providers_and_hosts_get_a_user_agent` |
| telemetry gate | `get_default_attribution_headers` | `telemetry_disabled_suppresses_attribution_headers` |
| OpenCode session headers + provider/host gate | `get_session_headers` | `opencode_session_headers_are_added_and_overridable`, `session_headers_are_limited_*` |
| `mergeProviderAttributionHeaders` (`Object.assign` order) | `merge_provider_attribution_headers` | override tests |

Gaps: the SDK-level capture path (session streaming) is not ported here; the
pure merge is. Header maps are `BTreeMap` (sorted) rather than insertion-ordered;
JSON object key order is not semantically observable.

## Q18 native session header input (2026-10-22)

Attribution helpers take native session IDs; only construction of the concrete
ProviderHeaders value encodes x-opencode-session. Raw-unit header fixture written
but unexecuted while coding compilation is blocked. SDK no longer projects a
native session ID to scalar text before calling attribution helpers.
