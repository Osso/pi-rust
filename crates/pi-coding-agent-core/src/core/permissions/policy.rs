//! Approval policies (`core/permissions/policy.ts`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    OnRequest,
    Never,
    AutoApprove,
}

pub const APPROVAL_POLICIES: [ApprovalPolicy; 3] = [
    ApprovalPolicy::OnRequest,
    ApprovalPolicy::Never,
    ApprovalPolicy::AutoApprove,
];

impl ApprovalPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnRequest => "on-request",
            Self::Never => "never",
            Self::AutoApprove => "auto-approve",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        APPROVAL_POLICIES.into_iter().find(|policy| policy.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum ApprovalPolicyDecision {
    Allow,
    Review,
    Block { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPolicyContext {
    pub approval_required: bool,
}

/// String-only form of `isApprovalPolicy`; non-string inputs are excluded by type.
pub fn is_approval_policy(value: &str) -> bool {
    ApprovalPolicy::parse(value).is_some()
}

/// `None` stands for any missing or non-string value.
pub fn normalize_approval_policy(value: Option<&str>) -> ApprovalPolicy {
    value
        .and_then(ApprovalPolicy::parse)
        .unwrap_or(ApprovalPolicy::OnRequest)
}

pub fn evaluate_approval_policy(policy: ApprovalPolicy, context: ApprovalPolicyContext) -> ApprovalPolicyDecision {
    if !context.approval_required {
        return ApprovalPolicyDecision::Allow;
    }
    match policy {
        ApprovalPolicy::Never => ApprovalPolicyDecision::Block {
            reason: "Blocked by approval policy: never".to_owned(),
        },
        ApprovalPolicy::AutoApprove => ApprovalPolicyDecision::Allow,
        ApprovalPolicy::OnRequest => ApprovalPolicyDecision::Review,
    }
}
