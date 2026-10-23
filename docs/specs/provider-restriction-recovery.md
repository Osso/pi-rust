# Codex restriction recovery

Frozen source: `core/provider-restriction-recovery.ts` at the [common pin](../porting/source-lock.json).

- Advisory requests require nonblank original request and refusal text, preserve evidence without truncation, and reject JSON contexts beyond 8000 UTF-16 units.
- Forward the caller's signal to the shared Supervisor requester, timeout 45000ms, one attempt. Cancellation before or after the requester blocks recovery.
- Accept only strict blocked/rescope objects. Reject normalized original-task repetitions and non-substring authorization quotes. Supervisor advice grants no execution permission.
- Format the literal tool-free main-model validation prompt, including text-only failure evidence and the full proposal.
- Validation requires normal stop, no nonempty error or tool content, strict four-field JSON, exact task equality and a nonblank exact authorization quote for allowed decisions. Denials retain their reason.
- Typed assistant content excludes malformed JavaScript runtime values; external advisory/validation JSON remains dynamically checked.

## Original-request persistence

Supervised main sessions record a fresh request UUID and exact original user text after extension interception and before template expansion, stream queuing or authentication. Handled input, extension-origin input, standalone workers and child/identity-required sessions do not record it. Transformed input retains the pre-transform authorization evidence. Storage failures propagate. Advisory and execution paths require the frozen identity and cancellation guards below.

## Guarded execution lifecycle

- [x] Consult once per persisted explicit request; the durable claim prevents repeat flags from repeatedly rescoping.
- [x] Validate the proposal on the unchanged model with no tools; preserve the refusal and original request. Invalid/denied/tool-bearing validation performs no operative call and restores the original tools.
- [x] Run at most one validated bounded attempt. Its transient/quota/overflow errors cause no retry, provider fallback or compaction resend; another restriction cannot obtain another advisory claim.
- [x] Emit codex_restriction_validation only for the validation agent_end and match frozen willRetry. Preserve its tool-revision discrepancy: policy-allowed validation can report willRetry=true even though the later revision guard prevents execution.
- [x] Reject stale model/message references, request/goal changes and queued input at the advisory callback boundary; abort, input reservation and explicit review cancellation synchronously abort its signal. Harmless metadata/supervisor-status appends remain eligible. Native guards use the [shared reference authority](agent-reference-identity.md), never equality.
- [x] Discard independently delayed advice after abort/reservation/review cancellation, model/goal/request/message change or disposal; queued corrections continue without executing stale advice.
- [x] Discard independently delayed validation after abort, reservation, model/message replacement or active-tool revision changes, without overwriting a new tool selection. The session cwd has no frozen post-construction assignment; direct JavaScript private-field mutation is outside the typed native API.
- [x] Restore tools after validation or operative timeout/abort without replay; a fresh explicit human request subsequently retries a transient error normally.

Native execution state lives in `core/agent_session/restriction.rs`, a child of the existing AgentSession module. The actual-frozen session oracle observes requester kinds, per-call tool sets, model, pending count, assistant/custom/tool-result contents and agent_end continuation/willRetry fields for twenty-three scenarios, including eligible quota fallback, overflow and active-tool revision changes. This is bounded lifecycle proof, not whole transcript equivalence or final acceptance.

Quota fallback order and the literal case-insensitive quota matcher follow `core/codex-quota-fallback.ts`. Only error-stop messages with truthy matching error text qualify. These helpers do not switch models or execute advice.
