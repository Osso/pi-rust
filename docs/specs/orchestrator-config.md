# Orchestrator config

Port of frozen `packages/orchestrator/src/config.ts`. Native boundary decisions and proof scope: [Q3 proof](../porting/orchestrator-proof.md).

## What it must do
- [x] isBunBinary, VERSION: native flag and frozen package version in differential config matrix.
- [x] getOrchestratorDir; getAuthPath/getMachinePath/getInstancesPath/getSocketPath: default/empty/explicit precedence, relative dot-segment normalization and HOME-absent OS lookup: foundation + differential.

## How it works
- [Module traceability](../porting/traceability/orchestrator-config.md)

## Implementation inventory
- `crates/pi-orchestrator/src/config.rs`: native module.

## Tests asserting this spec
- `crates/pi-orchestrator/tests/foundation.rs`
- `crates/pi-orchestrator/tests/differential.rs`

## Known gaps (current cycle)
No additional in-slice gap beyond the proof boundary above.

## Out of scope
Live services, user state and real Radius network calls; native packaging follows the approved library boundary.
