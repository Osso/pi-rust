# Traceability: `packages/coding-agent/src/core/permissions/orchestrator.ts`

Rust: `crates/pi-coding-agent-core/src/core/permissions/orchestrator.rs`. Tests:
`tests/permissions_prompt.rs`.

Pinned source: commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Exported surface

| TypeScript | Rust | Tests |
|---|---|---|
| `ApprovalReviewer` | `ApprovalReviewer` (`Arc<dyn Fn() -> BoxFuture<'static, Option<ToolCallEventResult>>>`) | every orchestration test |
| `ApprovalOrchestratorOptions` | `ApprovalOrchestratorOptions` | every orchestration test |
| `orchestrateToolApproval` | `orchestrate_tool_approval` | 7 tests |

## Branches

| TypeScript branch | Rust | Tests |
|---|---|---|
| policy `allow` | `ApprovalPolicyDecision::Allow` → `None` | `allows_auto_approve_...`, `allows_actions_that_do_not_require_approval` |
| policy `block` | `Block { reason }` → `{block:true, reason}` | `blocks_never_without_invoking_the_reviewer` |
| policy `review` | `Review` | remaining tests |
| hook reviewer decides | early return | `skips_the_llm_reviewer_when_a_hook_reviewer_decides` |
| LLM reviewer | awaited after hook | `falls_through_...`, `uses_the_llm_reviewer_...` |
| generic reviewer / none | final fallthrough | `routes_on_request_approvals_to_the_reviewer` |

## Gaps

- The source test cases for built-in `read`/`grep` `approvalRequired` are not
  ported because those tool definitions live in the in-flight `ca-tools-core`
  slice. The policy path they exercise is covered by
  `allows_actions_that_do_not_require_approval`.
