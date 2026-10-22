use pi_js_compat::js_json::JsString;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use pi_agent_core::harness::{
    BranchSummaryError, BranchSummaryErrorCode, CompactionError, CompactionErrorCode, ExecutionError,
    ExecutionErrorCode, FileError, FileErrorCode,
};
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_000;
const FIRST_COMPACTION_REQUEST_ID: u64 = 13_036;
const EXPECTED_EXISTING_FIXTURE_COUNT: usize = 14;
const EXPECTED_NEW_FIXTURE_COUNT: usize = 10;
const EXPECTED_FIXTURE_COUNT: usize = EXPECTED_EXISTING_FIXTURE_COUNT + EXPECTED_NEW_FIXTURE_COUNT;

#[derive(Clone, Copy)]
enum HarnessErrorKind {
    File,
    Execution,
    Compaction,
    BranchSummary,
}

struct Fixture {
    request_id: u64,
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
    Compaction {
        code: &'static str,
        message: &'static str,
        cause: Option<Box<CauseFixture>>,
    },
    BranchSummary {
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

pub(crate) fn compare_existing_fixtures(oracle: &mut TsOracle) {
    let fixtures = existing_fixtures();
    assert_eq!(fixtures.len(), EXPECTED_EXISTING_FIXTURE_COUNT);
    assert_request_ids(&fixtures, FIRST_REQUEST_ID);
    compare_fixtures(oracle, &fixtures);
}

pub(crate) fn compare_compaction_fixtures(oracle: &mut TsOracle) {
    let existing_fixture_count = existing_fixtures().len();
    let fixtures = compaction_fixtures();
    assert_eq!(existing_fixture_count + fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_eq!(fixtures.len(), EXPECTED_NEW_FIXTURE_COUNT);
    assert!(FIRST_COMPACTION_REQUEST_ID > FIRST_REQUEST_ID + EXPECTED_EXISTING_FIXTURE_COUNT as u64);
    assert_request_ids(&fixtures, FIRST_COMPACTION_REQUEST_ID);
    compare_fixtures(oracle, &fixtures);
}

fn compare_fixtures(oracle: &mut TsOracle, fixtures: &[Fixture]) {
    for fixture in fixtures {
        let expected = oracle.request(&request(fixture.request_id, fixture));
        let actual = rust_response(fixture.request_id, fixture);

        assert_eq!(actual, expected, "harness-error mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture], first_request_id: u64) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected_id = first_request_id + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected_id, "fixture IDs must remain ordered");
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

fn build_error(fixture: &Fixture) -> Box<dyn StdError + Send + Sync + 'static> {
    let cause = fixture.cause.as_ref().map(build_cause);
    match fixture.kind {
        HarnessErrorKind::File => Box::new(FileError::new(
            file_code(fixture.code),
            fixture.message,
            owned_path(fixture.path).map(JsString::from),
            cause,
        )),
        HarnessErrorKind::Execution => Box::new(ExecutionError::new(
            execution_code(fixture.code),
            fixture.message,
            cause,
        )),
        HarnessErrorKind::Compaction => Box::new(CompactionError::new(
            compaction_code(fixture.code),
            fixture.message,
            cause,
        )),
        HarnessErrorKind::BranchSummary => Box::new(BranchSummaryError::new(
            branch_summary_code(fixture.code),
            fixture.message,
            cause,
        )),
    }
}

fn build_cause(cause: &CauseFixture) -> Box<dyn StdError + Send + Sync + 'static> {
    match cause {
        CauseFixture::Ordinary { message } => Box::new(TestCause {
            message: (*message).to_owned(),
        }),
        CauseFixture::Execution { code, message, cause } => Box::new(ExecutionError::new(
            execution_code(code),
            *message,
            cause.as_deref().map(build_cause),
        )),
        CauseFixture::Compaction { code, message, cause } => Box::new(CompactionError::new(
            compaction_code(code),
            *message,
            cause.as_deref().map(build_cause),
        )),
        CauseFixture::BranchSummary { code, message, cause } => Box::new(BranchSummaryError::new(
            branch_summary_code(code),
            *message,
            cause.as_deref().map(build_cause),
        )),
    }
}

fn normalize_error(error: &(dyn StdError + 'static)) -> Value {
    if let Some(error) = error.downcast_ref::<FileError>() {
        return normalize_file_error(error);
    }
    if let Some(error) = error.downcast_ref::<ExecutionError>() {
        return normalize_execution_error(error);
    }
    if let Some(error) = error.downcast_ref::<CompactionError>() {
        return normalize_compaction_error(error);
    }
    if let Some(error) = error.downcast_ref::<BranchSummaryError>() {
        return normalize_branch_summary_error(error);
    }
    normalize_unknown_error(error)
}

fn normalize_file_error(error: &FileError) -> Value {
    json!({
        "kind": "FileError",
        "name": error.name,
        "message": error.message,
        "code": error.code.as_str(),
        "path": error.path,
        "cause": normalize_source(error.source()),
    })
}

fn normalize_execution_error(error: &ExecutionError) -> Value {
    json!({
        "kind": "ExecutionError",
        "name": error.name,
        "message": error.message,
        "code": error.code.as_str(),
        "path": Value::Null,
        "cause": normalize_source(error.source()),
    })
}

fn normalize_compaction_error(error: &CompactionError) -> Value {
    json!({
        "kind": "CompactionError",
        "name": error.name,
        "message": error.message,
        "code": error.code.as_str(),
        "path": Value::Null,
        "cause": normalize_source(error.source()),
    })
}

fn normalize_branch_summary_error(error: &BranchSummaryError) -> Value {
    json!({
        "kind": "BranchSummaryError",
        "name": error.name,
        "message": error.message,
        "code": error.code.as_str(),
        "path": Value::Null,
        "cause": normalize_source(error.source()),
    })
}

fn normalize_unknown_error(error: &(dyn StdError + 'static)) -> Value {
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
            typed_cause_request("execution", code, message, None, cause.as_deref())
        }
        CauseFixture::Compaction { code, message, cause } => {
            typed_cause_request("compaction", code, message, None, cause.as_deref())
        }
        CauseFixture::BranchSummary { code, message, cause } => {
            typed_cause_request("branch_summary", code, message, None, cause.as_deref())
        }
    }
}

fn typed_cause_request(
    kind: &str,
    code: &str,
    message: &str,
    path: Option<&str>,
    cause: Option<&CauseFixture>,
) -> Value {
    let mut request = json!({
        "kind": kind,
        "code": code,
        "message": message,
    });
    if let Some(path) = path {
        request["path"] = json!(path);
    }
    if let Some(cause) = cause {
        request["cause"] = cause_request(cause);
    }
    request
}

fn existing_fixtures() -> Vec<Fixture> {
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
        file_fixture(13_000, "file aborted empty", "aborted", "", None, None),
        file_fixture(
            13_001,
            "file not found ASCII path",
            "not_found",
            "missing",
            Some("/tmp/missing"),
            None,
        ),
        file_fixture(
            13_002,
            "file is directory NUL",
            "is_directory",
            "is\0directory",
            None,
            None,
        ),
        file_fixture(
            13_003,
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
            13_004,
            "file permission Unicode cause",
            "permission_denied",
            "权限失败",
            None,
            Some(ordinary_cause("permission cause")),
        ),
        file_fixture(
            13_005,
            "file not directory supplementary path and cause",
            "not_directory",
            "🙂 file",
            Some("目录/🙂"),
            Some(ordinary_cause("not-directory cause")),
        ),
        file_fixture(
            13_006,
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
        13_007,
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
    let mut fixtures = execution_boundary_fixtures();
    fixtures.extend(execution_cause_fixtures());
    fixtures.extend(execution_tail_fixtures());
    fixtures
}

fn execution_boundary_fixtures() -> Vec<Fixture> {
    vec![
        execution_fixture(13_008, "execution aborted empty", "aborted", "", None),
        execution_fixture(13_009, "execution timeout ASCII", "timeout", "timed out", None),
    ]
}

fn execution_cause_fixtures() -> Vec<Fixture> {
    vec![
        execution_fixture(
            13_010,
            "execution shell unavailable Unicode cause",
            "shell_unavailable",
            "shell unavailable 世界",
            Some(ordinary_cause("shell cause")),
        ),
        execution_fixture(
            13_011,
            "execution spawn supplementary cause",
            "spawn_error",
            "spawn failed 🙂",
            Some(ordinary_cause("spawn cause\0")),
        ),
    ]
}

fn execution_tail_fixtures() -> Vec<Fixture> {
    vec![
        execution_fixture(
            13_012,
            "execution callback NUL",
            "callback_error",
            "callback\0failed",
            None,
        ),
        execution_fixture(
            13_013,
            "execution unknown punctuation cause",
            "unknown",
            "unknown !@#$",
            Some(ordinary_cause("unknown cause")),
        ),
    ]
}

fn compaction_fixtures() -> Vec<Fixture> {
    let mut fixtures = compaction_value_fixtures();
    fixtures.extend(branch_summary_value_fixtures());
    fixtures.extend(cross_class_fixtures());
    fixtures
}

fn compaction_value_fixtures() -> Vec<Fixture> {
    vec![
        compaction_fixture(13_036, "compaction aborted empty", "aborted", "", None),
        compaction_fixture(
            13_037,
            "compaction summarization Unicode",
            "summarization_failed",
            "summarization failed 世界🙂",
            None,
        ),
        compaction_fixture(
            13_038,
            "compaction aborted NUL cause",
            "aborted",
            "compaction aborted",
            Some(ordinary_cause("cause\0compaction")),
        ),
        compaction_fixture(
            13_039,
            "compaction summarization punctuation cause",
            "summarization_failed",
            "compaction failed",
            Some(ordinary_cause("cause: compaction!?.,")),
        ),
    ]
}

fn branch_summary_value_fixtures() -> Vec<Fixture> {
    vec![
        branch_summary_fixture(13_040, "branch summary aborted empty", "aborted", "", None),
        branch_summary_fixture(
            13_041,
            "branch summary summarization Unicode",
            "summarization_failed",
            "branch summary failed 世界🙂",
            None,
        ),
        branch_summary_fixture(
            13_042,
            "branch summary aborted NUL cause",
            "aborted",
            "branch summary aborted",
            Some(ordinary_cause("cause\0branch")),
        ),
        branch_summary_fixture(
            13_043,
            "branch summary summarization punctuation cause",
            "summarization_failed",
            "branch summary failed",
            Some(ordinary_cause("cause: branch!?.,")),
        ),
    ]
}

fn cross_class_fixtures() -> Vec<Fixture> {
    vec![
        compaction_fixture(
            13_044,
            "compaction nested branch summary Unicode",
            "aborted",
            "compaction outer 世界🙂",
            Some(branch_summary_cause(
                "aborted",
                "branch summary nested",
                Some(ordinary_cause("root cause 世界🙂")),
            )),
        ),
        branch_summary_fixture(
            13_045,
            "branch summary nested compaction NUL",
            "summarization_failed",
            "branch summary outer\0",
            Some(compaction_cause(
                "aborted",
                "compaction nested",
                Some(ordinary_cause("root cause\0")),
            )),
        ),
    ]
}

fn file_fixture(
    request_id: u64,
    name: &'static str,
    code: &'static str,
    message: &'static str,
    path: Option<&'static str>,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        request_id,
        name,
        kind: HarnessErrorKind::File,
        code,
        message,
        path,
        cause,
    }
}

fn execution_fixture(
    request_id: u64,
    name: &'static str,
    code: &'static str,
    message: &'static str,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        request_id,
        name,
        kind: HarnessErrorKind::Execution,
        code,
        message,
        path: None,
        cause,
    }
}

fn compaction_fixture(
    request_id: u64,
    name: &'static str,
    code: &'static str,
    message: &'static str,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        request_id,
        name,
        kind: HarnessErrorKind::Compaction,
        code,
        message,
        path: None,
        cause,
    }
}

fn branch_summary_fixture(
    request_id: u64,
    name: &'static str,
    code: &'static str,
    message: &'static str,
    cause: Option<CauseFixture>,
) -> Fixture {
    Fixture {
        request_id,
        name,
        kind: HarnessErrorKind::BranchSummary,
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

fn compaction_cause(code: &'static str, message: &'static str, cause: Option<CauseFixture>) -> CauseFixture {
    CauseFixture::Compaction {
        code,
        message,
        cause: cause.map(Box::new),
    }
}

fn branch_summary_cause(code: &'static str, message: &'static str, cause: Option<CauseFixture>) -> CauseFixture {
    CauseFixture::BranchSummary {
        code,
        message,
        cause: cause.map(Box::new),
    }
}

fn fixture_kind(kind: HarnessErrorKind) -> &'static str {
    match kind {
        HarnessErrorKind::File => "file",
        HarnessErrorKind::Execution => "execution",
        HarnessErrorKind::Compaction => "compaction",
        HarnessErrorKind::BranchSummary => "branch_summary",
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

fn compaction_code(code: &str) -> CompactionErrorCode {
    match code {
        "aborted" => CompactionErrorCode::Aborted,
        "summarization_failed" => CompactionErrorCode::SummarizationFailed,
        _ => panic!("fixture must use a CompactionErrorCode literal"),
    }
}

fn branch_summary_code(code: &str) -> BranchSummaryErrorCode {
    match code {
        "aborted" => BranchSummaryErrorCode::Aborted,
        "summarization_failed" => BranchSummaryErrorCode::SummarizationFailed,
        _ => panic!("fixture must use a BranchSummaryErrorCode literal"),
    }
}
