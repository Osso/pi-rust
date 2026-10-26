# Host relay resident

Frozen: `packages/coding-agent/src/core/host-relay-resident.ts`, common pin `4f9e6f8a4`. Rust: `core/host_relay_resident.rs`. [Contract](../../specs/host-relay.md).

| Export / branch | Process-boundary proof (`host_relay_resident`) |
|---|---|
| ensureRelayRunning; absent settings | `absent_settings_spawn_nothing_and_invalid_identity_refuses_unverified_process`: no identity/log/SSH child |
| Shared startup lock, single launch, same-version reuse | `concurrent_startup_creates_one_resident_reuses_version_and_replaces_old_version`: three concurrent callers, one actual detached process/fake SSH |
| Verified old-version replacement; PID+start ticks identity | same test changes persisted version, next startup replaces PID, old identity is dead |
| Refuse own/unverified process | absent-settings test supplies current test-process identity with old version and receives refusal without signalling it |
| Startup wait does not block executor/session readiness | `startup_lock_is_shared_and_awaiting_it_does_not_block_executor`: shared proper-lockfile is held across async sleep, release lets child finish |
| runRelayConnect; global-only settings, lock ownership, identity write, signals/final cleanup | real CLI `relay connect` in client test; TERM removes identity and stops SSH; no-settings succeeds without child |
| Native launch, home cwd/env, private modes, append log | real harness copied as `pi` exercises the compiled-native branch and verified replacement; `setsid` supplies detached session; no TS-loader launch path in native binary |
| Invalid stored identity, startup timeout/lock exhaustion | malformed identity rejection tested; dedicated timeout/lock-exhaustion branches pending |

Main call site starts the helper as an error-logged background task after startup diagnostics, and skips `--no-supervisor`. Parser/diagnostics and early invalid standalone rejection have focused proof. Full standalone orchestration/handoff suppression is a separate Q14 main/CLI gap, not a relay semantic deviation. See [main](main.md).

`resident_identity_bytes_and_reuse_refusal_errors_match_frozen_bidirectionally` writes identity via frozen TS, reads/rewrites via Rust with exact byte equality, then lets frozen ensure reuse it without spawning. Old own/unverified PID and malformed JSON/identity errors match exactly; both runtimes release the shared startup lock. Frozen `VERSION` is `0.80.3`, native embedded package version is `0.84.1`: each implementation compares against its own runtime config as frozen code requires. Global package metadata re-pin is outside this slice; no relay-local version substitution is introduced.

Tests use private settings/control DBs and fake SSH only. Final mutation/check evidence: [relay proof ledger](../host-relay-proof.md).

## Retained UTF-8 boundary (2026-10-26)

`read_relay_identity` retains strict decoding of internal machine-written `.relay.json` process metadata; invalid bytes require corruption. Frozen `host-relay-resident.ts:27` uses `readFileSync(..., "utf8")`; excluded OS/data-fault case, not equivalent invalid-byte behavior. See the [complete file-read audit](../utf8-decoding-sweep.md).

## Harness intermittent (acceptance 2026-10-26)

`host_relay_resident::spawn_failure_is_logged_like_frozen_without_executing_any_ssh` failed once in the full workspace run with ETXTBSY ("Text file busy") when executing its freshly written fake script, then passed 5/5 targeted reruns. This is the write-then-exec race with concurrent forks in the multithreaded test process; no product path is involved.
