# Supervisor main — re-pin in progress

Whole frozen module read: `packages/coding-agent/src/supervisor/main.ts` at `4f9e6f8a4`. Rust: `crates/pi-coding-agent/src/supervisor/main.rs`.

| Frozen symbol / branch | Re-pin status |
| --- | --- |
| Model ID/provider order and `requireSupervisorModels` | Uses gpt-6.1-sol and filters primary/fallback providers by configured auth. No-auth/ordered-fallback actual-source fixture RED before port (`/tmp/runtime-services-red-supervisor-auth-model.log`); old model selection RED (`/tmp/runtime-services-red-supervisor-model.log`). GREEN pending. |
| Queue/settings/file gating/extension validation | Existing implementations; current branch audit/proofs pending. |
| `createSupervisorInstructionEvaluator`, `createSupervisorInstructionReviewer`, instruction veto/feedback | Service helpers now available; main integration still open. |
| Console snapshot/runtime identity and `openSupervisorSession` | Identity metadata/live resident preparation still open. |
| Resource loader/session/service lifecycle | Old-only remote-compact inline extension and old service wiring still open. |
| Request loop/evaluation/current-response reading | Prompt mismatch fixed in service; main cancellation/error/veto and response branches still open. |

Parent-approved fixture repair (2026-10-25): both frozen/native normalizers validate v4 UUID syntax only at `codex_restriction_context.data.requestId`, then canonicalize that generated field. The full custom record and prompt remain exact; no entry or assertion is removed. Existing generated id/parentId/toolCallId/time/CWD normalization is unchanged. Native faux model fixture now uses the frozen model ID.

Model fixture correction: the frozen generated catalog does not contain gpt-6.1-sol, so auth alone cannot make that ID available. The ordered-auth fixture now explicitly supplies valid matching models.json definitions for all three providers, without changing any expected selection/error assertion. The scripted request-loop fixture already supplies that model through its faux provider. Default-model/session and full request-loop differentials pass at f49da13ff; model/auth matrix rerun pending.
