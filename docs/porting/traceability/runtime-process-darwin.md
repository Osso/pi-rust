# Darwin process information — gap

Frozen `packages/coding-agent/src/core/runtime-process-darwin.ts` at `4f9e6f8a4`.
The lane assignment explicitly classifies Darwin as a gap.

| Frozen symbol/branch | Status |
| --- | --- |
| `DarwinProcessInfo`, `DarwinProcessBinding` | gap: native Darwin interface |
| `loadDarwinProcessBinding` cache, arm64/x64 selection, package path, export validation | gap |
| `isDarwinProcessInfo` pid/start/group/state checks | gap |
| `readDarwinProcessInfo` native query and invalid-info error | gap |

Linux process behavior and proofs: [runtime-process](runtime-process.md).
