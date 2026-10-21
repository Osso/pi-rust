# Orchestrator types

Port of frozen `packages/orchestrator/src/types.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] InstanceStatus: all five concrete serde wire values in foundation.
- [x] MachineRecord, InstanceRecord, RadiusRegistration: required/optional camelCase fields and omitted options: foundation; serialized records compared to TS storage fixtures.

## How it works
- [Module traceability](../porting/traceability/orchestrator-types.md)

## Implementation inventory
- `crates/pi-orchestrator/src/types.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
