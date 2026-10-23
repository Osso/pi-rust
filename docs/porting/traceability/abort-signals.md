# utils/abort-signals traceability

Pinned source: `packages/ai/src/utils/abort-signals.ts` at `4f9e6f8a4`.

The former `532f7d12` token/bridge-task proof is stale: it cannot preserve reasons or synchronous listener ordering. The new native carrier and direct frozen multi-source differential are documented in [shared abort](shared-abort.md).

| Frozen branch | Current proof/status |
|---|---|
| No active signals: undefined signal, no-op cleanup | Production wrapper migration and new-pin proof pending pi-ai lane |
| One active signal: original signal, no-op cleanup | Production wrapper migration and new-pin proof pending pi-ai lane |
| Many: immediate first pre-abort; synchronous first reason forwarding | Shared-carrier differential in `pi-js-compat/tests/abort_signal.rs` imports frozen module directly; wrapper migration pending |
| cleanup removes listeners | Shared native subscription detach is proven; wrapper cleanup integration pending |

No row is closed by stale token-only tests. Runtime scheduling and loss of `signal.reason` are not approved deviations.
