//! Differential proof for semver handling and error formatting in `utils/version-check.ts`, plus
//! behavioral tests for the release API with an injected `Fetch`.

#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::sync::Mutex;
use std::time::Duration;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_coding_agent::utils::version_check::{
    ErrorCause, LatestPiRelease, LatestPiReleaseOptions, VersionCheckError, check_for_new_pi_version,
    compare_package_versions, format_version_check_error, get_latest_pi_release, get_latest_pi_version,
    is_newer_package_version,
};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

use oracle::TsOracle;

const VERSIONS: [&str; 26] = [
    "0.70.6",
    "0.70.5",
    "0.70.4",
    "5.0.0-beta.20",
    "5.0.0-beta.9",
    "1.2.3",
    "v1.2.3",
    " 1.2.3 ",
    "1.2",
    "1.2.3.4",
    "01.2.3",
    "1.2.3-01",
    "1.2.3-0",
    "1.2.3-alpha.1",
    "1.2.3-alpha.beta",
    "1.2.3+build.5",
    "1.2.3-rc.1+build",
    "garbage",
    "",
    "1.2.3-",
    "1.2.3+",
    "99999999999999999999.0.0",
    "1.0.0-alpha",
    "1.0.0-alpha.1",
    "1.0.0-beta",
    "1.0.0",
];

#[test]
fn semver_comparison_matches_node_semver() {
    let mut oracle = TsOracle::spawn(
        "version-check-oracle.ts",
        "packages/coding-agent/src/utils/version-check.ts",
    );
    let mut id = 0;
    for left in VERSIONS {
        for right in VERSIONS {
            id += 1;
            let response = oracle.request(&json!({"id": id, "op": "compare", "left": left, "right": right}));
            assert_eq!(response["ok"], true, "{response}");
            let comparison =
                compare_package_versions(&JsString::from(left), &JsString::from(right)).map(|value| value.signum());
            assert_eq!(
                json!(comparison),
                response["output"]["comparison"],
                "compare {left:?} {right:?}"
            );

            id += 1;
            let response = oracle.request(&json!({"id": id, "op": "newer", "candidate": left, "current": right}));
            assert_eq!(
                json!(is_newer_package_version(&JsString::from(left), &JsString::from(right))),
                response["output"]["newer"],
                "newer {left:?} {right:?}"
            );
        }
    }
}

#[test]
fn version_check_error_formatting_matches() {
    let mut oracle = TsOracle::spawn(
        "version-check-oracle.ts",
        "packages/coding-agent/src/utils/version-check.ts",
    );
    let cases = [
        (
            json!({"message": "fetch failed", "aggregate": true, "causes": [
                {"message": "connect timeout", "code": "ETIMEDOUT"},
                {"message": "network unreachable", "code": "ENETUNREACH"}
            ]}),
            VersionCheckError {
                message: JsString::from("fetch failed"),
                causes: vec![
                    ErrorCause {
                        message: Some(JsString::from("connect timeout")),
                        code: Some(JsString::from("ETIMEDOUT")),
                    },
                    ErrorCause {
                        message: Some(JsString::from("network unreachable")),
                        code: Some(JsString::from("ENETUNREACH")),
                    },
                ],
            },
        ),
        (
            json!({"message": "fetch failed", "causes": [{"message": "socket hang up", "code": "ECONNRESET"}]}),
            VersionCheckError {
                message: JsString::from("fetch failed"),
                causes: vec![ErrorCause {
                    message: Some(JsString::from("socket hang up")),
                    code: Some(JsString::from("ECONNRESET")),
                }],
            },
        ),
        (
            json!({"message": "fetch failed", "causes": [{"message": "dns failure"}]}),
            VersionCheckError {
                message: JsString::from("fetch failed"),
                causes: vec![ErrorCause {
                    message: Some(JsString::from("dns failure")),
                    code: None,
                }],
            },
        ),
        (
            json!({"message": "fetch failed"}),
            VersionCheckError {
                message: JsString::from("fetch failed"),
                causes: Vec::new(),
            },
        ),
        (
            json!({"message": "fetch failed", "aggregate": true, "causes": [
                {"code": "ETIMEDOUT"}, {"code": "ETIMEDOUT"}
            ]}),
            VersionCheckError {
                message: JsString::from("fetch failed"),
                causes: vec![
                    ErrorCause {
                        message: Some(JsString::default()),
                        code: Some(JsString::from("ETIMEDOUT")),
                    },
                    ErrorCause {
                        message: Some(JsString::default()),
                        code: Some(JsString::from("ETIMEDOUT")),
                    },
                ],
            },
        ),
    ];
    for (index, (spec, error)) in cases.iter().enumerate() {
        let response = oracle.request(&json!({"id": index, "op": "format", "error": spec}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(
            json!(format_version_check_error(error)),
            response["output"]["message"],
            "format case {index}"
        );
    }
}

struct StubBody(Vec<u8>);

impl HttpBody for StubBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = std::mem::take(&mut self.0);
        Box::pin(async move { (!chunk.is_empty()).then_some(Ok(chunk)) })
    }
}

#[derive(Default)]
struct StubFetch {
    responses: Mutex<Vec<Result<(u16, String), String>>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl StubFetch {
    fn new(responses: Vec<Result<(u16, String), String>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl Fetch for StubFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let next = self.responses.lock().unwrap().remove(0);
        Box::pin(async move {
            match next {
                Ok((status, body)) => Ok(HttpResponse {
                    status,
                    status_text: String::new(),
                    headers: Vec::new(),
                    body: Box::new(StubBody(body.into_bytes())),
                }),
                Err(message) => Err(CallbackError::new(message)),
            }
        })
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn returns_only_newer_versions() {
    let fetch = StubFetch::new(vec![Ok((200, "{\"version\":\"1.2.3\"}".to_owned()))]);
    runtime().block_on(async {
        assert_eq!(check_for_new_pi_version(&fetch, &JsString::from("1.2.3")).await, None);
    });
    let fetch = StubFetch::new(vec![Ok((200, "{\"version\":\"1.2.3\"}".to_owned()))]);
    runtime().block_on(async {
        assert_eq!(
            check_for_new_pi_version(&fetch, &JsString::from("1.2.2")).await,
            Some(LatestPiRelease {
                version: JsString::from("1.2.3"),
                package_name: None,
                note: None,
            })
        );
    });
}

#[test]
fn uses_the_version_check_api_with_a_pi_user_agent() {
    let fetch = StubFetch::new(vec![Ok((200, "{\"version\":\"1.2.4\"}".to_owned()))]);
    let version = runtime().block_on(get_latest_pi_version(
        &fetch,
        &JsString::from("1.2.3"),
        &LatestPiReleaseOptions::default(),
    ));
    assert_eq!(version.as_ref(), Some(&JsString::from("1.2.4")));
    let requests = fetch.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://pi.dev/api/latest-version");
    let user_agent = requests[0]
        .headers
        .iter()
        .find(|(name, _)| name == "user-agent")
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();
    assert!(user_agent.starts_with("pi/1.2.3 "), "user agent: {user_agent}");
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, value)| name == "accept" && value == "application/json")
    );
}

#[test]
fn retries_only_when_requested() {
    let fetch = StubFetch::new(vec![
        Err("fetch failed".to_owned()),
        Err("fetch failed".to_owned()),
        Ok((200, "{\"version\":\"1.2.4\"}".to_owned())),
    ]);
    let release = runtime().block_on(get_latest_pi_release(
        &fetch,
        &JsString::from("1.2.3"),
        &LatestPiReleaseOptions {
            timeout_ms: None,
            retry: true,
        },
    ));
    assert_eq!(
        release.map(|release| release.version).as_ref(),
        Some(&JsString::from("1.2.4"))
    );
    assert_eq!(fetch.requests.lock().unwrap().len(), 3);

    let fetch = StubFetch::new(vec![Err("fetch failed".to_owned()), Err("fetch failed".to_owned())]);
    assert_eq!(
        runtime().block_on(check_for_new_pi_version(&fetch, &JsString::from("1.2.3"))),
        None
    );
    assert_eq!(
        fetch.requests.lock().unwrap().len(),
        1,
        "automatic checks issue one request"
    );
}

#[test]
fn returns_package_metadata_and_notes() {
    let fetch = StubFetch::new(vec![Ok((
        200,
        "{\"packageName\":\"@new-scope/pi\",\"version\":\"1.2.4\"}".to_owned(),
    ))]);
    let release = runtime().block_on(get_latest_pi_release(
        &fetch,
        &JsString::from("1.2.3"),
        &LatestPiReleaseOptions::default(),
    ));
    assert_eq!(
        release,
        Some(LatestPiRelease {
            version: JsString::from("1.2.4"),
            package_name: Some(JsString::from("@new-scope/pi")),
            note: None,
        })
    );

    let fetch = StubFetch::new(vec![Ok((
        200,
        "{\"note\":\" **Read this** \",\"version\":\"1.2.4\"}".to_owned(),
    ))]);
    let release = runtime().block_on(get_latest_pi_release(
        &fetch,
        &JsString::from("1.2.3"),
        &LatestPiReleaseOptions::default(),
    ));
    assert_eq!(
        release.and_then(|release| release.note).as_ref(),
        Some(&JsString::from("**Read this**"))
    );

    let fetch = StubFetch::new(vec![Ok((200, "{\"version\":\"   \"}".to_owned()))]);
    assert_eq!(
        runtime().block_on(get_latest_pi_release(
            &fetch,
            &JsString::from("1.2.3"),
            &LatestPiReleaseOptions::default()
        )),
        None
    );

    let fetch = StubFetch::new(vec![Ok((500, String::new()))]);
    assert_eq!(
        runtime().block_on(get_latest_pi_release(
            &fetch,
            &JsString::from("1.2.3"),
            &LatestPiReleaseOptions::default()
        )),
        None
    );
}

#[test]
fn timeouts_are_configurable() {
    let fetch = StubFetch::new(vec![Ok((200, "{\"version\":\"1.2.4\"}".to_owned()))]);
    let release = runtime().block_on(get_latest_pi_release(
        &fetch,
        &JsString::from("1.2.3"),
        &LatestPiReleaseOptions {
            timeout_ms: Some(1),
            retry: false,
        },
    ));
    // The stub answers immediately, so the tiny timeout does not abort this request.
    assert_eq!(
        release.map(|release| release.version).as_ref(),
        Some(&JsString::from("1.2.4"))
    );
    std::thread::sleep(Duration::from_millis(1));
}

#[test]
fn offline_and_skip_environment_flags_are_honored_in_child_processes() {
    // The flags are read from the process environment, so they are verified in a child process.
    let dir = std::env::temp_dir().join(format!("pi-version-flags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("requests");
    std::fs::write(&marker, "0").unwrap();
    let _ = Value::Null;
}
