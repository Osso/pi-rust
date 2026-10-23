# Traceability: agent-core node.ts

Authority: frozen `packages/agent-core/src/node.ts`, `4f9e6f8a4`. Native `pi-agent-core/src/node.rs` forwards the crate root and `harness::env::NodeExecutionEnv`, exactly the two frozen export groups.

`public_entrypoints::frozen_root_types_and_node_entrypoint_support_the_same_filesystem_contract` imports NodeExecutionEnv through node.rs and supporting option/content/result types through the root, exercising real create/write/read/remove/temp operations. RED/GREEN root-forward logs recorded in [index](agent-index.md). No Node-specific alternate runtime exists.

Entry-point forwarding is complete. POSIX environment behavior and approved Windows branch gaps belong to [Node env](harness-nodejs-env.md), not to the forwarding module.
