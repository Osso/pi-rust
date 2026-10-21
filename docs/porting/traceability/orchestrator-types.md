# Orchestrator types

Source: `packages/orchestrator/src/types.ts` at the [common pin](../source-lock.json). Rust: `crates/pi-orchestrator/src/types.rs`.
Status: **done**. [Q3 proof and native approvals](../orchestrator-proof.md).

| Export/branch | Behavioral proof |
| --- | --- |
| InstanceStatus | all five concrete serde wire values in foundation |
| MachineRecord, InstanceRecord, RadiusRegistration | required/optional camelCase fields and omitted options: foundation; serialized records compared to TS storage fixtures |

Proof is bounded to the listed representable runtime/serialization cases; no exhaustive platform or mutation-coverage percentage is claimed.
