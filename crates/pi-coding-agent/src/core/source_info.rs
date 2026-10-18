use serde::{Deserialize, Serialize};

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
#[serde(rename_all = "lowercase")]
pub enum SourceOrigin {
    Package,
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
    pub path: String,
    pub source: String,
    pub scope: SourceScope,
    pub origin: SourceOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<String>,
}

/// Approved Rust carrier for the anonymous TypeScript synthetic-source options shape.
#[derive(Debug)]
pub struct SyntheticSourceInfoOptions {
    pub source: String,
    pub scope: Option<SourceScope>,
    pub origin: Option<SourceOrigin>,
    pub base_dir: Option<String>,
}

/// Creates source metadata with the TypeScript synthetic-source defaults.
pub fn create_synthetic_source_info(path: &str, options: &SyntheticSourceInfoOptions) -> SourceInfo {
    SourceInfo {
        path: path.to_owned(),
        source: options.source.clone(),
        scope: options.scope.unwrap_or(SourceScope::Temporary),
        origin: options.origin.unwrap_or(SourceOrigin::TopLevel),
        base_dir: options.base_dir.clone(),
    }
}
