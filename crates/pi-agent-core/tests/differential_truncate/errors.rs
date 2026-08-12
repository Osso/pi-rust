use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use pi_agent_core::harness::{ExecutionError, ExecutionErrorCode, FileError, FileErrorCode};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_000;
const EXPECTED_FIXTURE_COUNT: usize = 14;

#[derive(Clone, Copy)]
enum HarnessErrorKind {
    File,
    Execution,
}

struct Fixture {
    name: &'static str,
    kind: HarnessErrorKind,
    code: &'static str,
    message: &'static str,
    path: Option<&'static str>,
    cause: Option<CauseFixture>,
}

enum CauseFixture {
    Ordinary {
        message: &'static str,
    },
    Execution {
        code: &'static str,
        message: &'static str,
        cause: Option<Box<CauseFixture>>,
    },
}

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

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);

    for (offset, fixture) in fixtures.iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&request(id, fixture));
        let actual = rust_response(id, fixture);

        assert_eq!(actual, expected, "harness-error mismatch: {}", fixture.name);
    }
}

fn request(id: u64, fixture: &Fixture) -> Value {
    let mut request = json!({
        "id": id,
        "operation": "harness_error",
        "kind": fixture_kind(fixture.kind),
        "code": fixture.code,
        "message": fixture.message,
    });
    if let Some(path) = fixture.path {
        request["path"] = json!(path);
    }
    if let Some(cause) = &fixture.cause {
        request["cause"] = cause_request(cause);
    }
    request
}

fn rust_response(id: u64, fixture: &Fixture) -> Value {
    let error = build_error(fixture);
    ok_response(id, normalize_error(error.as_ref()))
}

fn build_error(fixture: &Fixture) -> Box<dyn StdError + 'static> {
    let cause = fixture.cause.as_ref().map(build_cause);
    match fixture.kind {
        HarnessErrorKind::File => Box::new(FileError::new(
            file_code(fixture.code),
            fixture.message,
            owned_path(fixture.path),
            cause,
        )),
        HarnessErrorKind::Execution => Box::new(ExecutionError::new(
            execution_code(fixture.code),
            fixture.message,
            cause,
        )),
    }
}

fn build_cause(cause: &CauseFixture) -> Box<dyn StdError + 'static> {
    match cause {
        CauseFixture::Ordinary { message } => Box::new(TestCause {
            message: (*message).to_owned(),
        }),
        CauseFixture::Execution { code, message, cause } => Box::new(ExecutionError::new(
            execution_code(code),
            *message,
            cause.as_deref().map(build_cause),
        )),
    }
}

fn normalize_error(error: &(dyn StdError + 'static)) -> Value {
    if let Some(error) = error.downcast_ref::<FileError>() {
        return json!({
            "kind": "FileError",
            "name": error.name,
            "message": error.message,
            "code": error.code.as_str(),
            "path": error.path,
            "cause": normalize_source(error.source()),
        });
    }
    if let Some(error) = error.downcast_ref::<ExecutionError>() {
        return json!({
            "kind": "ExecutionError",
            "name": error.name,
            "message": error.message,
            "code": error.code.as_str(),
            "path": Value::Null,
            "cause": normalize_source(error.source()),
        });
    }

    json!({
        "kind": "Error",
        "name": "Error",
        "message": error.to_string(),
        "code": Value::Null,
        "path": Value::Null,
        "cause": normalize_source(error.source()),
    })
}

fn normalize_source(source: Option<&(dyn StdError + 'static)>) -> Value {
    source.map(normalize_error).unwrap_or(Value::Null)
}

fn cause_request(cause: &CauseFixture) -> Value {
    match cause {
        CauseFixture::Ordinary { message } => json!({
            "kind": "error",
            "message": message,
        }),
        CauseFixture::Execution { code, message, cause } => {
            let mut request = json!({
                "kind": "execution",
                "code": code,
                "message": message,
            });
            if let Some(cause) = cause {
                request["cause"] = cause_request(cause);
            }
            request
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = file_fixtures();
    fixtures.extend(execution_fixtures());
    fixtures
}

fn file_fixtures() -> Vec<Fixture> {
    let mut fixtures = file_boundary_fixtures();
    fixtures.extend(file_cause_fixtures());
    fixtures.push(file_nested_fixture());
    fixtures
}

fn file_boundary_fixtures() -> Vec<Fixture> {
    vec![
        file_fixture("file aborted empty", "aborted", "", None, None),
        file_fixture(
            "file not found ASCII path",
            "not_found",
            "missing",
            Some("/tmp/missing"),
            None,
        ),
        file_fixture("file is directory NUL", "is_directory", "is\0directory", None, None),
        file_fixture(
            "file invalid punctuation path",
            "invalid",
            "invalid!@#$%^&*()",
            Some("C:\\tmp\\file"),
            None,
        ),
    ]
}

fn file_cause_fixtures() -> Vec<Fixture> {
    vec![
        file_fixture(
            "file permission Unicode cause",
            "permission_denied",
            "权限失败",
            None,
            Some(ordinary_cause("permission cause")),
        ),
        file_fixture(
            "file not directory supplementary path and cause",
            "not_directory",
            "🙂 file",
            Some("目录/🙂"),
            Some(ordinary_cause("not-directory cause")),
        ),
        file_fixture(
            "file not supported cause",
            "not_supported",
            "not supported",
            None,
            Some(ordinary_cause("cause: unsupported")),
        ),
    ]
}

fn file_nested_fixture() -> Fixture {
    file_fixture(
        "file unknown nested typed chain",
        "unknown",
        "outer!",
        Some("/tmp/outer\0"),
        Some(execution_cause(
            "spawn_error",
            "spawn failed 🙂",
            Some(ordinary_cause("root cause [x]")),
        )),
    )
}

fn execution_fixtures() -> Vec<Fixture> {
    vec![
        execution_fixture("execution aborted empty", "aborted", "", None),
        execution_fixture("execution timeout ASCII", "timeout", "timed out", None),
        execution_fixture(
            "execution shell unavailable Unicode cause",
            "shell_unavailable",
            "shell unavailable 世界",
            Some(ordinary_cause("shell cause")),
        ),
        execution_fixture(
            "execution spawn supplementary cause",
            "spawn_error",
            "spawn failed 🙂",
            Some(ordinary_cause("spawn cause\0")),
        ),
        execution_fixture("execution callback NUL", "callback_error", "callback\0failed", None),
        execution_fixture(
            "execution unknown punctuation cause",
            "unknown",
            "unknown !@#$",
            Some(ordinary_cause("unknown cause")),
        ),
    ]
}

fn file_fixture(
    name: &'static str,
    code: &'static str,
    message: &'static str,
    path: Option<&'static str>,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        name,
        kind: HarnessErrorKind::File,
        code,
        message,
        path,
        cause,
    }
}

fn execution_fixture(
    name: &'static str,
    code: &'static str,
    message: &'static str,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        name,
        kind: HarnessErrorKind::Execution,
        code,
        message,
        path: None,
        cause,
    }
}

fn ordinary_cause(message: &'static str) -> CauseFixture {
    CauseFixture::Ordinary { message }
}

fn execution_cause(code: &'static str, message: &'static str, cause: Option<CauseFixture>) -> CauseFixture {
    CauseFixture::Execution {
        code,
        message,
        cause: cause.map(Box::new),
    }
}

fn fixture_kind(kind: HarnessErrorKind) -> &'static str {
    match kind {
        HarnessErrorKind::File => "file",
        HarnessErrorKind::Execution => "execution",
    }
}

fn owned_path(path: Option<&str>) -> Option<String> {
    path.map(str::to_owned)
}

fn file_code(code: &str) -> FileErrorCode {
    match code {
        "aborted" => FileErrorCode::Aborted,
        "not_found" => FileErrorCode::NotFound,
        "permission_denied" => FileErrorCode::PermissionDenied,
        "not_directory" => FileErrorCode::NotDirectory,
        "is_directory" => FileErrorCode::IsDirectory,
        "invalid" => FileErrorCode::Invalid,
        "not_supported" => FileErrorCode::NotSupported,
        "unknown" => FileErrorCode::Unknown,
        _ => panic!("fixture must use a FileErrorCode literal"),
    }
}

fn execution_code(code: &str) -> ExecutionErrorCode {
    match code {
        "aborted" => ExecutionErrorCode::Aborted,
        "timeout" => ExecutionErrorCode::Timeout,
        "shell_unavailable" => ExecutionErrorCode::ShellUnavailable,
        "spawn_error" => ExecutionErrorCode::SpawnError,
        "callback_error" => ExecutionErrorCode::CallbackError,
        "unknown" => ExecutionErrorCode::Unknown,
        _ => panic!("fixture must use an ExecutionErrorCode literal"),
    }
}
