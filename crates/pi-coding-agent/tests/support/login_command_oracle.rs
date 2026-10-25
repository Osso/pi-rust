#[path = "executable_script.rs"]
mod executable_script;
#[path = "oracle.rs"]
mod oracle;

use pi_ai::auth::AuthError;
use pi_ai::oauth::*;
use pi_coding_agent::cli::login_command::*;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_js_compat::js_json::{self, JsString};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

const INITIAL: &str = r#"{"keep":{"type":"api_key","key":"kept"}}"#;

fn cases() -> Vec<Value> {
    let mut cases = vec![
        json!({"args":["other"]}),
        json!({"args":["login"]}),
        json!({"args":["login",""]}),
        json!({"args":["login","--help"]}),
        json!({"args":["login","lane-login","-h"]}),
        json!({"args":["login","lane-login","--invalid"]}),
        json!({"args":["login","missing-provider"]}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\n2suffix\n"}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\nsecond\n"}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\n\n"}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\nbogus\n"}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\n1\n","emptyOptions":true}),
        json!({"args":["login","lane-login"],"input":"typed\nmanual\n1\n","fail":true}),
        json!({"args":["login","lane-login"],"input":"typed\r\nmanual\r\n2\r\n","stdio":true}),
        json!({"args":["login","lane-login"],"input":"typed\r","stdio":true}),
    ];
    for opener in ["success", "failure", "default"] {
        cases.push(json!({"args":["login","lane-login","--browser-cli"],"flow":"browser","opener":opener}));
    }
    cases
}

fn spawn_native(request: &Value, path: &std::ffi::OsStr, browser_log: &std::path::Path) -> std::process::Output {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "login_command_oracle_child", "--nocapture"])
        .env_clear()
        .env("PATH", path)
        .env("FORCE_COLOR", "0")
        .env("PI_LOGIN_CASE", request.to_string())
        .env("PI_BROWSER_OPEN_LOG", browser_log)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if request["stdio"].as_bool().unwrap_or(false) {
        input
            .write_all(request["input"].as_str().unwrap_or("").as_bytes())
            .unwrap();
    }
    drop(input);
    child.wait_with_output().unwrap()
}

pub fn compare(stdio: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("auth.json");
    let browser = directory.path().join("browser-cli");
    let browser_log = directory.path().join("browser.log");
    executable_script::write_executable_script(
        &browser,
        "#!/bin/sh\nprintf '%s\\n' \"$2\" > \"$PI_BROWSER_OPEN_LOG\"\nexit 7\n",
    );
    let paths = std::env::join_paths(
        std::iter::once(directory.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "login-command-oracle.ts",
        "packages/coding-agent/src/cli/login-command.ts",
        &[
            ("PATH", paths.clone()),
            ("FORCE_COLOR", "0".into()),
            ("PI_BROWSER_OPEN_LOG", browser_log.as_os_str().to_owned()),
        ],
    );
    for (index, mut request) in cases().into_iter().enumerate() {
        if request["stdio"].as_bool().unwrap_or(false) != stdio {
            continue;
        }
        request["id"] = json!(index);
        request["authPath"] = json!(path);
        std::fs::write(&path, INITIAL).unwrap();
        let expected = source.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let source_browser = std::fs::read(&browser_log).ok();
        if browser_log.exists() {
            std::fs::remove_file(&browser_log).unwrap();
        }
        std::fs::write(&path, INITIAL).unwrap();
        let child = spawn_native(&request, &paths, &browser_log);
        assert!(
            child.status.success(),
            "{}",
            String::from_utf8(child.stderr.clone()).unwrap()
        );
        let stdout = String::from_utf8(child.stdout).unwrap();
        let (_, body) = stdout.split_once("LOGIN_BEGIN\n").unwrap();
        let (stdio_output, result) = body.split_once("LOGIN_END:").unwrap();
        let mut actual: Value = serde_json::from_str(result.lines().next().unwrap()).unwrap();
        if request["stdio"].as_bool().unwrap_or(false) {
            actual["stdout"] = json!(stdio_output);
            actual["stderr"] = json!(String::from_utf8(child.stderr).unwrap());
        }
        assert_eq!(actual, expected["output"], "case={request}");
        if request["opener"] == "default" {
            assert_eq!(
                source_browser.as_deref(),
                Some(b"https://example.invalid/login\n".as_slice())
            );
            assert_eq!(std::fs::read(&browser_log).unwrap(), source_browser.unwrap());
        }
    }
}

pub fn compare_color_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("auth.json");
    std::fs::write(&path, INITIAL).unwrap();
    let request =
        json!({"id":1,"args":["login","missing-provider"],"withoutFixture":true,"stdio":true,"authPath":path});
    let mut source = oracle::TsOracle::spawn_with_environment(
        "login-command-oracle.ts",
        "packages/coding-agent/src/cli/login-command.ts",
        &[("FORCE_COLOR", "0".into())],
    );
    let expected = source.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    let quote = |value: &std::path::Path| format!("'{}'", value.to_str().unwrap().replace('\'', "'\\''"));
    let command = format!(
        "{} --ignored --exact login_command_oracle_child --nocapture > {}",
        quote(&std::env::current_exe().unwrap()),
        quote(&directory.path().join("stdout"))
    );
    let output = Command::new("script")
        .args(["-qefc", &command, "/dev/null"])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("TERM", "xterm-256color")
        .env("PI_LOGIN_CASE", request.to_string())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
    let expected = expected["output"]["stderr"].as_str().unwrap().replace('\n', "\r\n");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

fn register_fixture(request: &Value) {
    let browser = request["flow"] == "browser";
    let empty = request["emptyOptions"].as_bool().unwrap_or(false);
    let fail = request["fail"].as_bool().unwrap_or(false);
    super::register(
        "lane-login",
        "Lane Login",
        Box::new(move |callbacks| {
            Box::pin(async move {
                callbacks.on_auth(OAuthAuthInfo {
                    url: "https://example.invalid/login".into(),
                    instructions: Some("Open browser".into()),
                });
                if browser {
                    return Ok(OAuthCredentials::new(
                        "refresh-token",
                        "browser-access",
                        4_000_000_000_000.0,
                    ));
                }
                callbacks.on_device_code(OAuthDeviceCodeInfo {
                    verification_uri: "https://example.invalid/device".into(),
                    user_code: "CODE-1234".into(),
                    expires_in_seconds: Some(60.0),
                    interval_seconds: None,
                });
                callbacks.on_progress(&"Ready".into());
                let answer = callbacks
                    .on_prompt(OAuthPrompt {
                        message: "Token".into(),
                        placeholder: Some("hint".into()),
                        allow_empty: None,
                    })
                    .await
                    .map_err(AuthError::from)?;
                let manual = callbacks
                    .on_manual_code_input()
                    .unwrap()
                    .await
                    .map_err(AuthError::from)?;
                let options = if empty {
                    vec![]
                } else {
                    vec![
                        OAuthSelectOption {
                            id: "first".into(),
                            label: "First".into(),
                        },
                        OAuthSelectOption {
                            id: "second".into(),
                            label: "Second".into(),
                        },
                    ]
                };
                let selected = callbacks
                    .on_select(OAuthSelectPrompt {
                        message: "Account".into(),
                        options,
                    })
                    .await
                    .map_err(AuthError::from)?;
                if fail {
                    return Err(AuthError::failed("provider rejected"));
                }
                let access =
                    pi_js_compat::js_format!("{}|{}|{}", answer, manual, selected.unwrap_or_else(|| "none".into()));
                Ok(OAuthCredentials::new("refresh-token", access, 4_000_000_000_000.0))
            })
        }),
    );
}

pub async fn child() {
    let Ok(text) = std::env::var("PI_LOGIN_CASE") else {
        return;
    };
    let request: Value = serde_json::from_str(&text).unwrap();
    if !request["withoutFixture"].as_bool().unwrap_or(false) {
        register_fixture(&request);
    }
    let storage = AuthStorage::create(Some(&JsString::from(request["authPath"].as_str().unwrap()))).unwrap();
    let lines = request["input"].as_str().unwrap_or("").replace("\r\n", "\n");
    let lines: Vec<&str> = lines.split('\n').collect();
    let captured = super::TestIo::new(&lines);
    let io: Arc<dyn LoginIo> = if request["stdio"].as_bool().unwrap_or(false) {
        Arc::new(StdioLoginIo::new())
    } else {
        captured.clone()
    };
    let opener: Option<BrowserCliOpener> = if request["opener"] == "default" {
        None
    } else {
        let fail = request["opener"] == "failure";
        Some(Arc::new(move |_| {
            Box::pin(async move { if fail { Err("open rejected".into()) } else { Ok(()) } })
        }))
    };
    let args: Vec<String> = serde_json::from_value(request["args"].clone()).unwrap();
    println!("LOGIN_BEGIN");
    let outcome = handle_login_command(
        &args,
        LoginCommandDependencies {
            auth_storage: Some(storage.clone()),
            io: Some(io),
            use_browser_cli: None,
            open_browser_cli_url: opener,
        },
    )
    .await
    .unwrap();
    let providers: Vec<_> = storage
        .get_oauth_providers()
        .iter()
        .map(|provider| json!({"id":provider.id(),"name":provider.name()}))
        .collect();
    let result = pi_js_compat::js_json!({
        "handled":outcome.handled,"exitCode":outcome.exit_code,"stdout":captured.output(),"stderr":captured.error(),
        "file":std::fs::read_to_string(request["authPath"].as_str().unwrap()).unwrap(),"providers":providers,
    });
    println!("LOGIN_END:{}", js_json::stringify_lossless(&result));
}
