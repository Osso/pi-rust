use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use pi_agent_core::harness::{ExecutionError, ExecutionErrorCode, FileError, FileErrorCode};

#[derive(Debug)]
struct TestCause {
    message: String,
}

impl Display for TestCause {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl StdError for TestCause {}

fn assert_error_traits<T: StdError + Display + fmt::Debug>(_: &T) {}

#[test]
fn public_reexports_expose_every_source_error_code_literal() {
    let file_codes = [
        (FileErrorCode::Aborted, "aborted"),
        (FileErrorCode::NotFound, "not_found"),
        (FileErrorCode::PermissionDenied, "permission_denied"),
        (FileErrorCode::NotDirectory, "not_directory"),
        (FileErrorCode::IsDirectory, "is_directory"),
        (FileErrorCode::Invalid, "invalid"),
        (FileErrorCode::NotSupported, "not_supported"),
        (FileErrorCode::Unknown, "unknown"),
    ];
    let execution_codes = [
        (ExecutionErrorCode::Aborted, "aborted"),
        (ExecutionErrorCode::Timeout, "timeout"),
        (ExecutionErrorCode::ShellUnavailable, "shell_unavailable"),
        (ExecutionErrorCode::SpawnError, "spawn_error"),
        (ExecutionErrorCode::CallbackError, "callback_error"),
        (ExecutionErrorCode::Unknown, "unknown"),
    ];

    for (code, literal) in file_codes {
        assert_eq!(code.as_str(), literal);
    }
    for (code, literal) in execution_codes {
        assert_eq!(code.as_str(), literal);
    }
}

#[test]
fn file_error_minimal_constructor_preserves_public_fields_and_name() {
    let error = FileError::new(FileErrorCode::NotFound, "missing", None, None);

    assert_eq!(error.name, "FileError");
    assert_eq!(error.message, "missing");
    assert_eq!(error.code, FileErrorCode::NotFound);
    assert_eq!(error.path, None);
    assert!(error.cause.is_none());
    assert_eq!(error.to_string(), "missing");
    assert!(error.source().is_none());
    assert_error_traits(&error);
}

#[test]
fn file_error_preserves_empty_unicode_supplementary_and_nul_fields() {
    let error = FileError::new(FileErrorCode::Invalid, "", Some("路径/🙂\0file".to_owned()), None);

    assert_eq!(error.message, "");
    assert_eq!(error.path.as_deref(), Some("路径/🙂\0file"));
    assert_eq!(error.to_string(), "");
}

#[test]
fn file_error_preserves_a_typed_cause_and_nested_execution_error_chain() {
    let direct = FileError::new(
        FileErrorCode::Unknown,
        "outer",
        Some("/tmp/\0路径".to_owned()),
        Some(Box::new(TestCause {
            message: "inner cause".to_owned(),
        })),
    );
    assert_eq!(direct.source().map(ToString::to_string).as_deref(), Some("inner cause"));

    let nested_cause = ExecutionError::new(
        ExecutionErrorCode::SpawnError,
        "spawn failed",
        Some(Box::new(TestCause {
            message: "process cause".to_owned(),
        })),
    );
    let nested = FileError::new(FileErrorCode::Unknown, "outer", None, Some(Box::new(nested_cause)));
    let source = nested.source().expect("nested execution error must be the source");

    assert_eq!(source.to_string(), "spawn failed");
    assert_eq!(
        source.source().map(ToString::to_string).as_deref(),
        Some("process cause")
    );
}

#[test]
fn execution_error_constructor_preserves_public_fields_and_causes() {
    let without_cause = ExecutionError::new(ExecutionErrorCode::Timeout, "timed out", None);
    assert_eq!(without_cause.name, "ExecutionError");
    assert_eq!(without_cause.message, "timed out");
    assert_eq!(without_cause.code, ExecutionErrorCode::Timeout);
    assert!(without_cause.cause.is_none());
    assert_eq!(without_cause.to_string(), "timed out");
    assert!(without_cause.source().is_none());
    assert_error_traits(&without_cause);

    let with_cause = ExecutionError::new(
        ExecutionErrorCode::CallbackError,
        "callback failed",
        Some(Box::new(TestCause {
            message: "callback cause".to_owned(),
        })),
    );
    assert_eq!(
        with_cause.source().map(ToString::to_string).as_deref(),
        Some("callback cause")
    );
}

#[test]
fn public_field_mutation_changes_display_and_error_source() {
    let mut file_error = FileError::new(FileErrorCode::Aborted, "before", None, None);
    file_error.name = "名前🙂\0".to_owned();
    file_error.message = "after🙂\0".to_owned();
    file_error.code = FileErrorCode::PermissionDenied;
    file_error.path = Some("/changed".to_owned());
    file_error.cause = Some(Box::new(TestCause {
        message: "changed cause".to_owned(),
    }));

    assert_eq!(file_error.name, "名前🙂\0");
    assert_eq!(file_error.message, "after🙂\0");
    assert_eq!(file_error.code, FileErrorCode::PermissionDenied);
    assert_eq!(file_error.path.as_deref(), Some("/changed"));
    assert_eq!(file_error.to_string(), "after🙂\0");
    assert_eq!(
        file_error.source().map(ToString::to_string).as_deref(),
        Some("changed cause")
    );

    let mut execution_error = ExecutionError::new(ExecutionErrorCode::Aborted, "before", None);
    execution_error.message = "after".to_owned();
    execution_error.code = ExecutionErrorCode::Unknown;
    execution_error.cause = Some(Box::new(TestCause {
        message: "execution cause".to_owned(),
    }));

    assert_eq!(execution_error.to_string(), "after");
    assert_eq!(execution_error.code, ExecutionErrorCode::Unknown);
    assert_eq!(
        execution_error.source().map(ToString::to_string).as_deref(),
        Some("execution cause")
    );
}
