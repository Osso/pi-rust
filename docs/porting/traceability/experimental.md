# coding-agent experimental traceability

Pinned source: `packages/coding-agent/src/core/experimental.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: focused child-process env probes in `crates/pi-coding-agent/tests/experimental.rs` (edition 2024 forbids `set_var`, so each case runs the test binary with a controlled environment). Differential testing is not applicable: the source reads `process.env` at call time, which the TS oracle process cannot vary per case without mutating its own environment.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `PREFER_STRICT_TOOL_SAMPLING` | `{type:"json_schema",strict:"prefer"}` | `ConstrainedSamplingConfig::JsonSchema { strict: Prefer }` (reuses `pi-ai` wire types) |
| `areExperimentalFeaturesEnabled` | True only when `PI_EXPERIMENTAL === "1"` | `are_experimental_features_enabled`; probes for unset, empty, `1`, `0`, `true` |
| `getExperimentalToolSampling` | Strict sampling config when enabled, otherwise `undefined` | `get_experimental_tool_sampling() -> Option<ConstrainedSamplingConfig>`; probe asserts `null` versus `{"type":"json_schema","strict":"prefer"}` |

## Exclusions

- The return type is `Option<ConstrainedSamplingConfig>` instead of the source's untyped object literal; the serialized value is identical.

## Discrepancies

None.
