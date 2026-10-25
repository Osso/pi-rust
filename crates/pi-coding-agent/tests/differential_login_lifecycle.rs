#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::components::login_dialog::LoginDialogComponent;
use pi_js_compat::js_json::JsString;
use pi_tui::{Component, Focusable};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Answer = Pin<Box<dyn Future<Output = Value>>>;

fn describe_reason(reason: Option<pi_js_compat::abort::AbortReason>) -> Value {
    use pi_js_compat::{abort::AbortReason, js_json::JsValue};
    let Some(reason) = reason else {
        return Value::Null;
    };
    let error = reason.as_error().unwrap();
    json!({"name":error.name,"message":error.message,"code":error.value().get("code").and_then(JsValue::as_f64).unwrap() as i64,"dom":matches!(reason,AbortReason::DomException(_))})
}

#[test]
fn login_prompt_lifetimes_and_empty_optionals_match_frozen() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = tempfile::tempdir().unwrap();
    let launcher = fixture.path().join("xdg-open");
    std::fs::write(&launcher, "#!/usr/bin/python3\nimport json, os, sys\nwith open(os.environ['PI_LOGIN_BROWSER_LOG'], 'a') as output:\n    output.write(json.dumps(sys.argv[1:]) + '\\n')\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let log = fixture.path().join("browser.jsonl");
    let path = format!("{}:{}", fixture.path().display(), std::env::var("PATH").unwrap());
    env_probe::run_probe(
        "login_lifecycle_probe",
        &[
            ("PATH", path.into()),
            ("PI_LOGIN_BROWSER_LOG", log.as_os_str().to_owned()),
        ],
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let calls: Vec<Value> = std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        if calls.len() == 4 {
            let mut actual: Vec<_> = calls
                .into_iter()
                .map(|call| call[0].as_str().unwrap().to_owned())
                .collect();
            actual.sort();
            let mut expected = vec![
                "https://fixture.invalid/?a=1&b=|;".to_owned(),
                "https://fixture.invalid/?a=1&b=|;".to_owned(),
                "https://fixture.invalid/instructions".to_owned(),
                "https://fixture.invalid/instructions".to_owned(),
            ];
            expected.sort();
            assert_eq!(actual, expected);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "browser stub did not receive four launches: {calls:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
#[ignore = "child process probe"]
fn login_lifecycle_probe() {
    if !env_probe::in_probe() {
        return;
    }
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
    let scenarios = [
        json!([{"op":"prompt","message":"Empty optional","placeholder":""},{"op":"input","data":"answer"},{"op":"input","data":"\r"},{"op":"progress","message":"progress"},{"op":"waiting","message":"waiting"}]),
        json!([{"op":"prompt","message":"First"},{"op":"prompt","message":"Second","placeholder":"example"},{"op":"input","data":"answer"},{"op":"input","data":"\r"},{"op":"manual","message":"Code"},{"op":"input","data":"\u{001b}"},{"op":"input","data":"\u{001b}"}]),
        json!([{"op":"auth","url":"https://fixture.invalid/?a=1&b=|;","instructions":""},{"op":"auth","url":"https://fixture.invalid/instructions","instructions":"Follow instructions"},{"op":"device","info":{"verificationUri":"https://fixture.invalid/device","userCode":"ABCD"}}]),
        json!([{"op":"info","lines":["one","two"]},{"op":"focus","value":true},{"op":"manual","message":"Code"},{"op":"input","data":"manual"},{"op":"input","data":"\r"},{"op":"info","lines":[]}]),
    ];
    let mut all_actual = Vec::new();
    let mut all_expected = Vec::new();
    for steps in scenarios {
        let expected=oracle.request(&json!({"id":1,"module":"modes/interactive/components/login-dialog.ts","operation":"loginLifecycle","width":50,"steps":steps}));
        assert_eq!(expected["ok"], true, "{expected}");
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let render_events = events.clone();
        let complete_events = events.clone();
        let mut dialog = LoginDialogComponent::new(
            Rc::new(move || render_events.borrow_mut().push(json!(["render"]))),
            "fixture",
            Rc::new(move |success, message| complete_events.borrow_mut().push(json!(["complete", success, message]))),
            None,
            Some(&"Fixture".into()),
        );
        let signal = dialog.signal();
        let aborts = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let observed = aborts.clone();
        signal.add_listener(move |reason| observed.lock().unwrap().push(describe_reason(Some(reason.clone()))));
        let mut pending = Vec::<Option<Answer>>::new();
        let mut answers = Vec::<Value>::new();
        let mut states = Vec::new();
        for step in steps.as_array().unwrap() {
            match step["op"].as_str().unwrap() {
                "prompt" | "manual" => {
                    let message = JsString::from(step["message"].as_str().unwrap());
                    let receiver = if step["op"] == "prompt" {
                        dialog.show_prompt(&message, step["placeholder"].as_str().map(JsString::from).as_ref())
                    } else {
                        dialog.show_manual_input(&message)
                    };
                    pending.push(Some(Box::pin(async move {
                        match receiver.await {
                            Ok(value) => json!(["ok", value]),
                            Err(error) => json!(["error", error.0]),
                        }
                    })));
                    answers.push(json!("pending"));
                }
                "auth" => dialog.show_auth(
                    &JsString::from(step["url"].as_str().unwrap()),
                    step["instructions"].as_str().map(JsString::from).as_ref(),
                ),
                "device" => dialog.show_device_code(&pi_ai::OAuthDeviceCodeInfo {
                    user_code: step["info"]["userCode"].as_str().unwrap().into(),
                    verification_uri: step["info"]["verificationUri"].as_str().unwrap().into(),
                    interval_seconds: None,
                    expires_in_seconds: None,
                }),
                "input" => dialog.handle_input(&JsString::from(step["data"].as_str().unwrap())),
                "info" => dialog.show_info(
                    &step["lines"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| JsString::from(v.as_str().unwrap()))
                        .collect::<Vec<_>>(),
                ),
                "waiting" => dialog.show_waiting(&JsString::from(step["message"].as_str().unwrap())),
                "progress" => dialog.show_progress(&JsString::from(step["message"].as_str().unwrap())),
                "focus" => dialog.set_focused(step["value"] == true),
                op => panic!("unknown login step {op}"),
            }
            for (index, answer) in pending.iter_mut().enumerate() {
                if let Some(future) = answer
                    && let Poll::Ready(value) = future.as_mut().poll(&mut Context::from_waker(Waker::noop()))
                {
                    answers[index] = value;
                    *answer = None;
                }
            }
            let frames: Vec<Vec<u16>> = dialog.render(50).iter().map(|line| line.units().to_vec()).collect();
            states.push(json!({"frames":frames,"answers":answers,"cancelled":dialog.signal().aborted(),"focused":dialog.focused(),"sameSignal":signal==dialog.signal(),"reason":describe_reason(signal.reason()),"aborts":aborts.lock().unwrap().clone()}));
        }
        all_actual.push(json!({"states":states,"events":*events.borrow()}));
        all_expected.push(expected["output"].clone());
    }
    assert_eq!(all_actual, all_expected);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
