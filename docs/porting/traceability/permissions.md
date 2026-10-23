# coding-agent permissions traceability

Pinned source: `packages/coding-agent/src/core/permissions/{policy,presets}.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: requests `27001` (policy), `27002` (policy evaluation), `27003` (presets) execute the pinned modules across every policy/preset/sandbox value plus 12 near-miss strings. Focused tests: `crates/pi-coding-agent/tests/permissions.rs`.

## policy.ts

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `APPROVAL_POLICIES` | `["on-request","never","auto-approve"]` | `APPROVAL_POLICIES: [ApprovalPolicy; 3]`; `accepts_only_distinct_supported_policy_presets`, differential `27001` |
| `ApprovalPolicy` | String union of the three presets | `enum ApprovalPolicy` with kebab-case serde; `as_str`/`parse`; differential `27001` |
| `ApprovalPolicyDecision` | `{action:"allow"} \| {action:"review"} \| {action:"block",reason}` | `enum ApprovalPolicyDecision` (`action` tag); serialization asserted in `routes_approval_required_actions_for_on_request_review`, differential `27002` |
| `ApprovalPolicyContext` | `{approvalRequired}` | `ApprovalPolicyContext { approval_required }` with camelCase wire name |
| `isApprovalPolicy` | String membership | `is_approval_policy(&str)`; non-string inputs are excluded by type (see Exclusions) |
| `normalizeApprovalPolicy` | Valid value, else `on-request` | `normalize_approval_policy(Option<&str>)`; `None` is the non-string/missing mapping; differential `27001` |
| `evaluateApprovalPolicy` | No approval → allow; `never` → block with exact reason; `auto-approve` → allow; else review | `evaluate_approval_policy`; focused cases, differential `27002` |

## presets.ts

Common-pin whole-module proof supersedes the prior-pin presets table: [permissions presets](permissions-presets.md). The policy.ts proof above remains prior-pin evidence only.

## Exclusions

- `isApprovalPolicy` is a typed `&str` predicate in this prior-pin proof. Current preset/profile predicate carriers and exclusions are documented only in [permissions presets](permissions-presets.md).

## Discrepancies

None.
