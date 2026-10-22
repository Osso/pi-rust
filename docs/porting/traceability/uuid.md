# agent-core session/uuid — common pin

Frozen: `packages/agent-core/src/harness/session/uuid.ts:1–54`, 4f9e6f8a4. Rust: `crates/pi-agent-core/src/harness/session/uuid.rs`, exported from `pi_agent_core::uuidv7` as frozen `index.ts:34`.

| Frozen branch / symbols | Native mapping and proof |
|---|---|
| `fillRandomBytes` :4–12, crypto-present path | Exactly pinned `getrandom` OS entropy; no weak fallback. Three public runtime tests check UUID layout, timestamp, monotonicity and uniqueness. |
| Module state and `uuidv7` :15–31 | Mutex-owned last timestamp / sequence; reseed on advance, increment on same/backward clock, u32 wrap increments timestamp. Two focused tests and fresh frozen VM vectors. |
| Timestamp/sequence byte packing :33–47 | Eight direct frozen VM vectors include negative timestamps, rollback, reseed, sequence overflow, >48-bit timestamps and max Date millis. Negative timestamps follow frozen division/truncation/bitwise conversion, not signed integer shifts. |
| `formatUuid` :50–54 | Exact lowercase hex, hyphens, version/variant and deterministic vector strings. |

GAP (unsupported environment): frozen :4–11 falls back to `Math.random` only when `globalThis.crypto` is absent. The frozen package requires Node `>=22.19.0` (`packages/agent-core/package.json`, `engines.node`); parent classifies this branch as unreachable in supported runtimes (2026-10-22). Rust has no equivalent JS-global realm: native UUIDs always use OS entropy, and entropy failures fail explicitly. No weak fallback is added.

DISCREPANCY preserved: negative timestamps use JS division before bitwise coercion. For timestamp -1 and zero entropy, frozen outputs `00000000-00ff-7000-8000-000000000000`, not a two's-complement all-FF timestamp. `uuid-negative-red.log` reproduced the old Rust mismatch; direct frozen vectors now match.

Proof: 4 module tests (including 8 deterministic frozen VM vectors), 3 public behavioral tests, and 1 live 200-ID frozen oracle test GREEN. Logs `/tmp/pi-rust-repin-delta-agent-core/uuid-{unit,oracle}-green.log`.

Ownership cleanup: deleted `pi_ai::utils::uuid` and the AI-root v7 export; moved v7 tests/oracle to agent-core. `pi_ai::utils::random_uuid::random_uuid` remains the native `crypto.randomUUID` v4 primitive. Codex websocket request IDs without a session now use v4 exactly as frozen `packages/ai/src/api/openai-codex-responses.ts:1613`; the real local-websocket regression test is RED→GREEN (`codex-uuid-{red,green}.log`).
