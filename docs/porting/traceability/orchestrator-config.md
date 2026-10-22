# Orchestrator config

Source: `packages/orchestrator/src/config.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/config.rs`.
Status: **done**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| isBunBinary, VERSION | native flag and frozen package version in differential config matrix |
| getOrchestratorDir; getAuthPath/getMachinePath/getInstancesPath/getSocketPath | default/empty/explicit precedence, relative dot-segment normalization and HOME-absent OS lookup: foundation + differential |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.
