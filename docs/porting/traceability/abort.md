# Retired abort helper trace

The prior trace described `packages/ai/src/utils/abort.ts` at `532f7d12`. That module does not exist at the common pin `4f9e6f8a4`; prior token-only helper tests are not current-pin parity evidence.

The shared reason-bearing boundary is specified in [shared abort](shared-abort.md). Existing old-only pi-ai/coding helper consumers require explicit migration/removal by their owning lanes; this note does not authorize keeping a default-reason fallback.
