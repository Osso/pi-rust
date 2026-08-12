use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Skill loaded from a `SKILL.md` file or provided by an application.
#[derive(Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    pub file_path: String,
    pub disable_model_invocation: Option<bool>,
}

/// Prompt template that can be formatted into a prompt for explicit invocation.
#[derive(Debug)]
pub struct PromptTemplate {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
}

/// Resources made available to explicit invocation methods and system-prompt callbacks.
#[derive(Debug)]
pub struct AgentHarnessResources {
    pub prompt_templates: Option<Vec<PromptTemplate>>,
    pub skills: Option<Vec<Skill>>,
}

/// Kind of filesystem object addressed by a `FileInfo` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

impl FileKind {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }
}

/// Metadata for one filesystem object.
#[derive(Debug)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: f64,
    pub mtime_ms: f64,
}

/// Stable, backend-independent filesystem failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorCode {
    Aborted,
    NotFound,
    PermissionDenied,
    NotDirectory,
    IsDirectory,
    Invalid,
    NotSupported,
    Unknown,
}

impl FileErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::NotFound => "not_found",
            Self::PermissionDenied => "permission_denied",
            Self::NotDirectory => "not_directory",
            Self::IsDirectory => "is_directory",
            Self::Invalid => "invalid",
            Self::NotSupported => "not_supported",
            Self::Unknown => "unknown",
        }
    }
}

/// Error returned by filesystem operations.
#[derive(Debug)]
pub struct FileError {
    pub name: String,
    pub message: String,
    pub code: FileErrorCode,
    pub path: Option<String>,
    pub cause: Option<Box<dyn Error + 'static>>,
}

impl FileError {
    pub fn new(
        code: FileErrorCode,
        message: impl Into<String>,
        path: Option<String>,
        cause: Option<Box<dyn Error + 'static>>,
    ) -> Self {
        Self {
            name: "FileError".to_owned(),
            message: message.into(),
            code,
            path,
            cause,
        }
    }
}

impl Display for FileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for FileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref()
    }
}

/// Stable, backend-independent process-execution failure codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorCode {
    Aborted,
    Timeout,
    ShellUnavailable,
    SpawnError,
    CallbackError,
    Unknown,
}

impl ExecutionErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aborted => "aborted",
            Self::Timeout => "timeout",
            Self::ShellUnavailable => "shell_unavailable",
            Self::SpawnError => "spawn_error",
            Self::CallbackError => "callback_error",
            Self::Unknown => "unknown",
        }
    }
}

/// Error returned by process-execution operations.
#[derive(Debug)]
pub struct ExecutionError {
    pub name: String,
    pub message: String,
    pub code: ExecutionErrorCode,
    pub cause: Option<Box<dyn Error + 'static>>,
}

impl ExecutionError {
    pub fn new(code: ExecutionErrorCode, message: impl Into<String>, cause: Option<Box<dyn Error + 'static>>) -> Self {
        Self {
            name: "ExecutionError".to_owned(),
            message: message.into(),
            code,
            cause,
        }
    }
}

impl Display for ExecutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ExecutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause.as_deref()
    }
}
