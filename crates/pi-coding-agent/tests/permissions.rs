use pi_coding_agent::core::permissions::policy::{
    APPROVAL_POLICIES, ApprovalPolicy, ApprovalPolicyContext, ApprovalPolicyDecision, evaluate_approval_policy,
    is_approval_policy, normalize_approval_policy,
};
use pi_coding_agent::core::permissions::presets::{
    APPROVAL_PRESETS, ApprovalPresetName, ApprovalPresetReviewer, SANDBOX_PROFILES, SandboxProfileName,
    approval_preset_for_policy, approval_preset_to_bypass_permissions, find_approval_preset, is_approval_preset_name,
    is_sandbox_profile_name,
};
use serde_json::json;

const REQUIRED: ApprovalPolicyContext = ApprovalPolicyContext {
    approval_required: true,
};

#[test]
fn accepts_only_distinct_supported_policy_presets() {
    assert!(
        ["on-request", "never", "auto-approve"]
            .iter()
            .all(|value| is_approval_policy(value))
    );
    assert!(!is_approval_policy("always"));
    assert_eq!(
        APPROVAL_POLICIES,
        [
            ApprovalPolicy::OnRequest,
            ApprovalPolicy::Never,
            ApprovalPolicy::AutoApprove
        ]
    );
}

#[test]
fn routes_approval_required_actions_for_on_request_review() {
    let decision = evaluate_approval_policy(ApprovalPolicy::OnRequest, REQUIRED);
    assert_eq!(decision, ApprovalPolicyDecision::Review);
    assert_eq!(serde_json::to_value(decision).unwrap(), json!({"action": "review"}));
}

#[test]
fn rejects_approval_required_actions_for_never_without_reviewer_involvement() {
    let decision = evaluate_approval_policy(ApprovalPolicy::Never, REQUIRED);
    assert_eq!(
        serde_json::to_value(decision).unwrap(),
        json!({"action": "block", "reason": "Blocked by approval policy: never"})
    );
}

#[test]
fn approves_approval_required_actions_for_auto_approve_without_reviewer_involvement() {
    assert_eq!(
        evaluate_approval_policy(ApprovalPolicy::AutoApprove, REQUIRED),
        ApprovalPolicyDecision::Allow
    );
}

#[test]
fn allows_actions_that_do_not_require_approval_for_every_policy() {
    for policy in APPROVAL_POLICIES {
        let decision = evaluate_approval_policy(
            policy,
            ApprovalPolicyContext {
                approval_required: false,
            },
        );
        assert_eq!(decision, ApprovalPolicyDecision::Allow);
    }
}

#[test]
fn normalizes_unknown_or_missing_policies_to_on_request() {
    assert_eq!(normalize_approval_policy(Some("never")), ApprovalPolicy::Never);
    assert_eq!(normalize_approval_policy(Some("Never")), ApprovalPolicy::OnRequest);
    assert_eq!(normalize_approval_policy(None), ApprovalPolicy::OnRequest);
    assert_eq!(ApprovalPolicy::AutoApprove.as_str(), "auto-approve");
}

#[test]
fn keeps_approval_presets_distinct_from_sandbox_profiles() {
    let names: Vec<&str> = APPROVAL_PRESETS.iter().map(|preset| preset.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "ask-me",
            "llm-approved-deny",
            "llm-approved-ask",
            "never-ask-deny",
            "auto-approve"
        ]
    );
    let policies: Vec<&str> = APPROVAL_PRESETS.iter().map(|preset| preset.policy.as_str()).collect();
    assert_eq!(
        policies,
        ["on-request", "on-request", "on-request", "never", "auto-approve"]
    );
    let reviewers: Vec<ApprovalPresetReviewer> = APPROVAL_PRESETS.iter().map(|preset| preset.reviewer).collect();
    assert_eq!(
        serde_json::to_value(reviewers).unwrap(),
        json!(["human", "llm-deny", "llm-ask", "none", "none"])
    );
    let profiles: Vec<&str> = SANDBOX_PROFILES.iter().map(|profile| profile.name.as_str()).collect();
    assert_eq!(profiles, ["read-only", "workspace-write", "full-access"]);
    assert_eq!(
        serde_json::to_value(SANDBOX_PROFILES[1]).unwrap(),
        json!({"name": "workspace-write", "label": "Default/Workspace Write"})
    );
}

#[test]
fn maps_presets_policies_and_bypass_flags() {
    let preset = find_approval_preset(ApprovalPresetName::LlmApprovedAsk);
    assert_eq!(
        serde_json::to_value(preset).unwrap(),
        json!({"name": "llm-approved-ask", "label": "LLM Approved (and ask)", "policy": "on-request", "reviewer": "llm-ask"})
    );
    assert_eq!(
        approval_preset_for_policy(ApprovalPolicy::Never),
        ApprovalPresetName::NeverAskDeny
    );
    assert_eq!(
        approval_preset_for_policy(ApprovalPolicy::AutoApprove),
        ApprovalPresetName::AutoApprove
    );
    assert_eq!(
        approval_preset_for_policy(ApprovalPolicy::OnRequest),
        ApprovalPresetName::AskMe
    );
    assert!(approval_preset_to_bypass_permissions(ApprovalPresetName::AutoApprove));
    assert!(!approval_preset_to_bypass_permissions(ApprovalPresetName::NeverAskDeny));
    assert!(is_approval_preset_name("never-ask-deny"));
    assert!(!is_approval_preset_name("Ask Me"));
    assert!(is_sandbox_profile_name("full-access"));
    assert!(!is_sandbox_profile_name("inherit"));
    assert_eq!(
        SandboxProfileName::parse("read-only"),
        Some(SandboxProfileName::ReadOnly)
    );
}
