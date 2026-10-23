# Traceability: agent-core index.ts

Authority: frozen `packages/agent-core/src/index.ts`, `4f9e6f8a4`; native `pi-agent-core/src/lib.rs` and `agent/mod.rs`.

| Frozen export group | Native public forwarding | Proof |
|---|---|---|
| Agent/loop/types/proxy wildcard exports | `agent::types::*`, Agent/options/hook structural records, loop/proxy entrypoints | `agent_types`, `agent`, `agent_loop`, `proxy` and direct frozen differentials; module-specific traceability |
| Harness runtime/types wildcard exports | `harness::agent_harness::*`, `harness::types::*` | `public_entrypoints` external-crate compilation plus real filesystem calls; working harness tests/differential |
| Explicit compaction/branch functions and constants | Explicit same-function forwards in lib.rs | `compaction`, `branch_summarization`, frozen tree-compaction differential |
| Messages/templates/skills/system prompt wildcard exports | Corresponding native module glob forwards | Native/raw deterministic frozen differentials in each module trace |
| Session repo/session/repo-utils exports and uuidv7 | `harness::session::*`, native uuidv7 forward | Native frozen session/repository matrices and UUID proof |
| Shell output/truncate wildcard exports | `harness::utils::*` | Native shell capture/truncation frozen differentials |

Root forwarding is complete independently of remaining runtime-row proof gaps. Rust overload/trait/option records express TS inline types; they are not compatibility runtime paths. Old-only telemetry/search/tools/legacy-session scaffolds were removed/re-homed by the owning slices, not retained as alternate root behavior.

RED `root-forwards-red.log` found seven missing supporting public types because lib.rs selectively forwarded harness/types despite frozen `export *`. GREEN `root-forwards-green.log`: root-imported options/content/result/patch types work with the node entrypoint through create/write/read/remove/temp operations. Native Result helpers map directly to Rust constructors/operators; [harness type map](agent-core-harness-types.md). Runtime semantics remain delegated to their module proofs rather than being inferred from compilation.
