# coding-agent experimental traceability

Frozen source: `packages/coding-agent/src/core/experimental.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (whole module: three lines).

| Export / branch | Rust mapping and proof |
| --- | --- |
| `areExperimentalFeaturesEnabled`: exact `process.env.PI_EXPERIMENTAL === "1"`, read on every invocation | `are_experimental_features_enabled`; standalone `tests/experimental.rs::experimental_gate_and_tool_sampling_match_frozen_source` runs private child processes for unset, empty, `0`, `true`, `1`, ` 1`, `01` |
| No sampling helpers or sampling changes in frozen module | Removed old-only Rust sampling helpers and bash/edit/write `AgentTool` overrides; same test compares all three native tools against actual frozen tool definitions, with each environment value |

Parent approved the cross-lane removal on 2026-10-25. `pi-ai` types are unchanged; their separate removal belongs to the AI lane.

RED: `/tmp/settings-models-experimental-red-final.log` fails for `PI_EXPERIMENTAL=1`: all three native tools returned json_schema/prefer instead of frozen null. The public wrapped factories already returned null, so the regression exercises the concrete native tools, not only wrappers. GREEN and targeted clippy are recorded in the lane proof ledger after the commit.

No semantic deviations. Pathological environment/runtime faults remain outside the user-observable parity bar.
