# `core/permissions/presets.ts`

Authority: complete 66-line module at the [common pin](../source-lock.json). Rust: `crates/pi-coding-agent-data/src/core/permissions/presets.rs`. [Contract](../../specs/sandbox-selector.md).

| Export | Native mapping | Proof |
|---|---|---|
| ApprovalPresetReviewer / ApprovalPreset / ApprovalPresetName | exact enum spellings and immutable record | complete serialized preset catalog against actual source |
| APPROVAL_PRESETS | five records with exact name/label/policy/reviewer | `approval_presets_match_typescript` |
| findApprovalPreset | immutable reference, total over typed name union | lookup for every name matches source records |
| isApprovalPresetName | single `&JsString` membership predicate | 15 scalar inputs including every preset/profile name; raw-unit, NUL and near-miss corpus |
| approvalPresetForPolicy | never → never-ask-deny, auto-approve → auto-approve, otherwise ask-me | all three typed policies |
| approvalPresetToBypassPermissions | only auto-approve | all five names |
| SandboxProfile / SandboxProfileName / SandboxProfileScope | immutable record and exact name/scope enums | serialized profile catalog; source selector callback events cover every scope/profile and absent session profile |
| SANDBOX_PROFILES / isSandboxProfileName | ordered three-profile catalog and single `&JsString` membership predicate | catalog, scalar names and raw-unit corpus |
| isSandboxPlatformSupported | explicit `&str` authority, equality to linux; selector default resolves process.platform spelling | source/native linux/darwin/win32/freebsd/aix/openbsd/sunos/android; actual source default and native mapped host default; unsupported-selector rendered labels |

## Reference/consumer audit

Frozen consumers: settings-manager.ts:544 reads the found preset policy; approval-selector.ts reads the array for labels, index lookup, length and selection; sandbox-selector.ts maps profile records to fresh label/profile options. None mutate returned records or catalogs. Native records remain immutable; no mutation-through-returned-reference behavior is needed under the general owned-value approval.

Typed name/policy/scope enums exclude invalid union values. Membership predicates accept raw UTF-16; unmatched units are valid string inputs and yield false. Only non-string JS runtime input types remain excluded by the existing approval. Settings JSON values now reach the single JsString predicate before conversion to validated ASCII enum names; CLI/approval-control scalar callers adapt at the callsite. Three real global/project settings-file scenarios prove unmatched-value rejection and original precedence. This is carrier alignment, not a claimed repair to old settings outputs: the former enum_literal path already rejected unmatched units. This whole presets file has no remaining export gap; permissions policy.ts keeps its separate prior-pin proof and is not reclassified here.

## Proof

- Expanded current-source `approval_presets_match_typescript` and `preset_membership_matches_frozen_for_raw_utf16` in `tests/differential_core_leaves.rs` via `oracle/coding-agent-core-leaves-oracle.ts`: GREEN2/2, log `test-sandbox-presets-green-3.log` under `/tmp/pi-rust-repin-lossless-interactive/`.
- Shared helper's missing unsupported-platform consumption was reproduced by actual frozen selector RED before helper/label production edits: `test-sandbox-selector-platform-red-1.log`; full source/native GREEN96 sequences: `test-sandbox-selector-green-1.log`.
- Real settings-file carrier/precedence regression GREEN1/1: `test-sandbox-settings-carrier-green-1.log`.
- [Selector traceability](sandbox-selector.md) covers complete inherited Container and render/input integration.
