//! Approval presets and sandbox profiles (`core/permissions/presets.ts`).

use serde::{Deserialize, Serialize};

use super::policy::ApprovalPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPresetReviewer {
    Human,
    LlmDeny,
    LlmAsk,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPresetName {
    AskMe,
    LlmApprovedDeny,
    LlmApprovedAsk,
    NeverAskDeny,
    AutoApprove,
}

impl ApprovalPresetName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AskMe => "ask-me",
            Self::LlmApprovedDeny => "llm-approved-deny",
            Self::LlmApprovedAsk => "llm-approved-ask",
            Self::NeverAskDeny => "never-ask-deny",
            Self::AutoApprove => "auto-approve",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        APPROVAL_PRESETS
            .iter()
            .map(|preset| preset.name)
            .find(|name| name.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ApprovalPreset {
    pub name: ApprovalPresetName,
    pub label: &'static str,
    pub policy: ApprovalPolicy,
    pub reviewer: ApprovalPresetReviewer,
}

pub const APPROVAL_PRESETS: [ApprovalPreset; 5] = [
    ApprovalPreset {
        name: ApprovalPresetName::AskMe,
        label: "Ask Me",
        policy: ApprovalPolicy::OnRequest,
        reviewer: ApprovalPresetReviewer::Human,
    },
    ApprovalPreset {
        name: ApprovalPresetName::LlmApprovedDeny,
        label: "LLM Approved (and deny)",
        policy: ApprovalPolicy::OnRequest,
        reviewer: ApprovalPresetReviewer::LlmDeny,
    },
    ApprovalPreset {
        name: ApprovalPresetName::LlmApprovedAsk,
        label: "LLM Approved (and ask)",
        policy: ApprovalPolicy::OnRequest,
        reviewer: ApprovalPresetReviewer::LlmAsk,
    },
    ApprovalPreset {
        name: ApprovalPresetName::NeverAskDeny,
        label: "Never Ask/Deny",
        policy: ApprovalPolicy::Never,
        reviewer: ApprovalPresetReviewer::None,
    },
    ApprovalPreset {
        name: ApprovalPresetName::AutoApprove,
        label: "Auto Approve",
        policy: ApprovalPolicy::AutoApprove,
        reviewer: ApprovalPresetReviewer::None,
    },
];

pub fn find_approval_preset(name: ApprovalPresetName) -> &'static ApprovalPreset {
    APPROVAL_PRESETS
        .iter()
        .find(|preset| preset.name == name)
        .expect("every ApprovalPresetName has a preset")
}

/// String-only form of `isApprovalPresetName`; non-string inputs are excluded by type.
pub fn is_approval_preset_name(value: &str) -> bool {
    ApprovalPresetName::parse(value).is_some()
}

pub fn approval_preset_for_policy(policy: ApprovalPolicy) -> ApprovalPresetName {
    match policy {
        ApprovalPolicy::Never => ApprovalPresetName::NeverAskDeny,
        ApprovalPolicy::AutoApprove => ApprovalPresetName::AutoApprove,
        ApprovalPolicy::OnRequest => ApprovalPresetName::AskMe,
    }
}

pub fn approval_preset_to_bypass_permissions(preset_name: ApprovalPresetName) -> bool {
    preset_name == ApprovalPresetName::AutoApprove
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxProfileName {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

impl SandboxProfileName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::FullAccess => "full-access",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        SANDBOX_PROFILES
            .iter()
            .map(|profile| profile.name)
            .find(|name| name.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SandboxProfile {
    pub name: SandboxProfileName,
    pub label: &'static str,
}

pub const SANDBOX_PROFILES: [SandboxProfile; 3] = [
    SandboxProfile {
        name: SandboxProfileName::ReadOnly,
        label: "Read Only",
    },
    SandboxProfile {
        name: SandboxProfileName::WorkspaceWrite,
        label: "Default/Workspace Write",
    },
    SandboxProfile {
        name: SandboxProfileName::FullAccess,
        label: "Full Access",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxProfileScope {
    Global,
    Project,
    Session,
}

/// String-only form of `isSandboxProfileName`; non-string inputs are excluded by type.
pub fn is_sandbox_profile_name(value: &str) -> bool {
    SandboxProfileName::parse(value).is_some()
}
