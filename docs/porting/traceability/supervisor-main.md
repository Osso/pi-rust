# Supervisor main — re-pin in progress

Checkpoint f7a7d4d58: warning-mode `cargo clippy -j3 -p pi-coding-agent --lib` exits 0, no diagnostics on changed lines; one old service-wiring redundant conversion remains outside this slice. Source inventory PASS: 686 rows / 186679 lines. Manual Rust/TS changed-line readability audit completed.

Whole frozen module read: `packages/coding-agent/src/supervisor/main.ts` at `4f9e6f8a4`. Rust: `crates/pi-coding-agent/src/supervisor/main.rs`.

| Frozen symbol / branch | Re-pin status |
| --- | --- |
| Model ID/provider order and `requireSupervisorModels` | Uses gpt-6.1-sol and filters primary/fallback providers by configured auth. No-auth/ordered-fallback actual-source fixture RED before port (`/tmp/runtime-services-red-supervisor-auth-model.log`); old model selection RED (`/tmp/runtime-services-red-supervisor-model.log`). GREEN at bf711e332 for no-auth and ordered provider selection (`/tmp/runtime-services-green-supervisor-auth-model.log`). |
| Queue/settings/file gating/extension validation | Existing implementations; current branch audit/proofs pending. |
| `createSupervisorInstructionEvaluator`, `createSupervisorInstructionReviewer`, instruction veto/feedback | Reviewer and request veto/feedback ported; default suppression differential RED `/tmp/runtime-services-red-supervisor-policy.log`, GREEN at 5ba09742b. Expanded actual-source matrix covers all six decisions, invalid token, evaluator/feedback failure, generic bypass, compaction threshold and blocked durable completion (`/tmp/runtime-services-supervisor-policy-matrix.log`). Stateless evaluator/service wiring still open. |
| Console snapshot/runtime identity and `openSupervisorSession` | Resident preparation/open comparison GREEN at c3dcce5e0 (`/tmp/runtime-services-green-supervisor-open.log`). Snapshot identity metadata still open. |
| Resource loader/session/service lifecycle | Old-only remote-compact inline extension and old service wiring still open. |
| Request loop/evaluation/current-response reading | Prompt mismatch fixed in service; request completion errors now propagate, cancelled requests are re-read before error completion, and evaluation awaits prompt cleanup after abort. End-turn/no-stopped-response GREEN at 5ba09742b. Cancellation during review RED proved that frozen removes the session-abort listener before awaiting the veto promise; corrected native listener lifetime, focused GREEN at f7a7d4d58 (`/tmp/runtime-services-green-supervisor-policy-cancel.log`). |

Parent-approved fixture repair (2026-10-25): both frozen/native normalizers validate v4 UUID syntax only at `codex_restriction_context.data.requestId`, then canonicalize that generated field. The full custom record and prompt remain exact; no entry or assertion is removed. Existing generated id/parentId/toolCallId/time/CWD normalization is unchanged. Native faux model fixture now uses the frozen model ID.

Model fixture correction: the frozen generated catalog does not contain gpt-6.1-sol, so auth alone cannot make that ID available. The ordered-auth fixture now explicitly supplies valid matching models.json definitions for all three providers, without changing any expected selection/error assertion. The scripted request-loop fixture already supplies that model through its faux provider. Default-model/session and full request-loop differentials pass at f49da13ff; model/auth matrix rerun pending.

Resident preparation RED `/tmp/runtime-services-red-supervisor-open.log`: old caller selected the latest file but left canonical/older files behind. open_supervisor_session now delegates to the already-proved resident preparation authority, uses default metadata options like source, and retains open/create/archive behavior. Actual-source session-open fixture compares retained files, selected basename, session ID and cwd; GREEN at c3dcce5e0 (`/tmp/runtime-services-green-supervisor-open.log`).
