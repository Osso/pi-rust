use pi_js_compat::js_json::JsString;
use serde::{Deserialize, Serialize};

use crate::core::package_manager::{MetadataOrigin, PathMetadata};

/// Scope of a source path in the coding-agent metadata model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceScope {
    User,
    Project,
    Temporary,
}

impl SourceScope {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Temporary => "temporary",
        }
    }
}

/// Origin of a source path in the coding-agent metadata model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceOrigin {
    #[serde(rename = "package")]
    Package,
    #[serde(rename = "top-level")]
    TopLevel,
}

impl SourceOrigin {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Package => "package",
            Self::TopLevel => "top-level",
        }
    }
}

/// Source metadata returned by the synthetic source-info constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub path: JsString,
    pub source: JsString,
    pub scope: SourceScope,
    pub origin: SourceOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<JsString>,
}

impl From<crate::core::package_manager::SourceScope> for SourceScope {
    fn from(scope: crate::core::package_manager::SourceScope) -> Self {
        match scope {
            crate::core::package_manager::SourceScope::User => Self::User,
            crate::core::package_manager::SourceScope::Project => Self::Project,
            crate::core::package_manager::SourceScope::Temporary => Self::Temporary,
        }
    }
}

impl From<MetadataOrigin> for SourceOrigin {
    fn from(origin: MetadataOrigin) -> Self {
        match origin {
            MetadataOrigin::Package => Self::Package,
            MetadataOrigin::TopLevel => Self::TopLevel,
        }
    }
}

/// Creates source metadata from package-manager path metadata (`createSourceInfo`).
pub fn create_source_info(path: &JsString, metadata: &PathMetadata) -> SourceInfo {
    SourceInfo {
        path: path.to_owned(),
        source: metadata.source.clone(),
        scope: metadata.scope.into(),
        origin: metadata.origin.into(),
        base_dir: metadata.base_dir.clone(),
    }
}

/// Approved Rust carrier for the anonymous TypeScript synthetic-source options shape.
#[derive(Debug)]
pub struct SyntheticSourceInfoOptions {
    pub source: JsString,
    pub scope: Option<SourceScope>,
    pub origin: Option<SourceOrigin>,
    pub base_dir: Option<JsString>,
}

/// Creates source metadata with the TypeScript synthetic-source defaults.
pub fn create_synthetic_source_info(path: &JsString, options: &SyntheticSourceInfoOptions) -> SourceInfo {
    SourceInfo {
        path: path.to_owned(),
        source: options.source.clone(),
        scope: options.scope.unwrap_or(SourceScope::Temporary),
        origin: options.origin.unwrap_or(SourceOrigin::TopLevel),
        base_dir: options.base_dir.clone(),
    }
}
