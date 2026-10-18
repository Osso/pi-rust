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

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `APPROVAL_PRESETS` | Five presets with exact names, labels, policies, reviewers | `APPROVAL_PRESETS: [ApprovalPreset; 5]`; `keeps_approval_presets_distinct_from_sandbox_profiles`, differential `27003` |
| `ApprovalPresetReviewer` | `human \| llm-deny \| llm-ask \| none` | `enum ApprovalPresetReviewer` (kebab-case) |
| `ApprovalPresetName` | Union of the five preset names | `enum ApprovalPresetName`; `parse`/`as_str` |
| `findApprovalPreset` | Preset by name (total over the union) | `find_approval_preset` returns `&'static ApprovalPreset`; differential `27003` |
| `isApprovalPresetName` | String membership | `is_approval_preset_name(&str)`; differential `27003` |
| `approvalPresetForPolicy` | `never` → `never-ask-deny`, `auto-approve` → `auto-approve`, else `ask-me` | `approval_preset_for_policy`; differential `27003` |
| `approvalPresetToBypassPermissions` | Only `auto-approve` | `approval_preset_to_bypass_permissions`; differential `27003` |
| `SANDBOX_PROFILES` | Three profiles with exact names/labels and no `policy` field | `SANDBOX_PROFILES: [SandboxProfile; 3]`; the TypeScript test's `!("policy" in profile)` is structural — the Rust record simply has no such field |
| `SandboxProfileName` / `SandboxProfileScope` | Name union; `global \| project \| session` | `enum SandboxProfileName`, `enum SandboxProfileScope` |
| `isSandboxProfileName` | String membership | `is_sandbox_profile_name(&str)`; differential `27003` |

## Exclusions

- `isApprovalPolicy`, `isApprovalPresetName`, and `isSandboxProfileName` are typed `&str` predicates. JavaScript callers can pass any runtime value; typed Rust callers cannot (approved August 12, 2026 decision).

## Discrepancies

None.
