# supervisor/approval-reviewer.ts

Frozen `4f9e6f8a4`; Rust `src/supervisor/approval_reviewer.rs`.

| Export / branch | Proof |
|---|---|
| `SupervisorApprovalCall`, `SupervisorHumanReviewer` | Native fallible closure aliases; all consumers compile |
| `reviewToolCallWithSupervisor`: approve, reject, optional human, rejection escalation, invalid decision, request/human rejection | `q16_approval::approval_escalation_and_rejections_match_frozen`: ten cases against actual frozen module; `approval_reviewer` behavioral tests |

No live reviewer/service. [Slice proof](../q16-supervisor-runtime-proof.md).
