//! Common-pin self-restart.ts:97-160 argument/environment boundary differential.
use crate::shared_oracle as oracle;
use pi_coding_agent::cli::args::Args;
use pi_coding_agent::core::self_restart::*;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn restart_consumption_matches_frozen_empty_numeric_and_environment_cleanup_branches() {
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for (session, old_pid, marker) in [
        ("", "100", "1"),
        ("/s", "", "1"),
        ("/s", "100", "1"),
        ("/s", "42", "1"),
        ("/s", "0x64", "1"),
        ("/s", "100.0", "1"),
        ("/s", "100.5", "1"),
        ("/s", "NaN", "1"),
        ("/s", "Infinity", "1"),
        ("/s", "9007199254740992", "1"),
        ("/s", "-1", "1"),
        ("/s", "999", "1"),
        ("/s", "100", "0"),
    ] {
        let mut env: BTreeMap<String, String> = [
            (ENV_SELF_RESTART_REQUEST, marker),
            (ENV_SELF_RESTART_SESSION, session),
            (ENV_SELF_RESTART_OLD_PID, old_pid),
            (ENV_SELF_RESTART_PROMPT, "notice"),
            ("unrelated", "keep"),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
        let expected = source.request(&json!({"op":"consume","env":env,"currentPid":100,"parentPid":42}));
        let handoff = consume_self_restart_request(&mut env, 100, 42).map(|handoff| {
            let mut value = json!({"sessionFile":handoff.session_file});
            if let Some(prompt) = handoff.prompt {
                value["prompt"] = json!(prompt);
            }
            if let Some(pid) = handoff.old_pid {
                value["oldPid"] = json!(pid);
            }
            value
        });
        assert_eq!(
            json!({"handoff":handoff,"env":env}),
            expected,
            "session {session:?}, pid {old_pid}, marker {marker}"
        );
    }
}

#[test]
fn restart_consumption_does_not_alias_out_of_range_pids_to_the_override_pid() {
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for raw in ["4294967296", "9007199254740991"] {
        let mut env: BTreeMap<String, String> = [
            (ENV_SELF_RESTART_REQUEST, "1"),
            (ENV_SELF_RESTART_SESSION, "/s"),
            (ENV_SELF_RESTART_OLD_PID, raw),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value.into()))
        .collect();
        let expected = source.request(&json!({"op":"consume","env":env,"currentPid":u32::MAX,"parentPid":42}));
        assert_eq!(expected, json!({"handoff":null,"env":{}}));
        assert_eq!(consume_self_restart_request(&mut env, u32::MAX, 42), None, "{raw}");
        assert!(env.is_empty());
    }
}

#[tokio::test]
async fn restart_wait_matches_frozen_zero_current_absent_and_exited_pid_branches() {
    use std::sync::{Arc, Mutex};
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for old_pid in [None, Some(0), Some(100), Some(42)] {
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut request = json!({"op":"wait","currentPid":100});
        let handoff = old_pid.map(|pid| SelfRestartHandoff {
            session_file: "/s".into(),
            prompt: None,
            old_pid: Some(pid),
        });
        if let Some(pid) = old_pid {
            request["handoff"] = json!({"sessionFile":"/s","oldPid":pid});
        }
        let expected = source.request(&request);
        let dependencies = SelfRestartWaitDependencies {
            current_pid: 100,
            is_process_alive: Arc::new({
                let calls = calls.clone();
                move |pid| {
                    calls.lock().unwrap().push(format!("alive:{pid}"));
                    false
                }
            }),
            now: Arc::new({
                let calls = calls.clone();
                move || {
                    calls.lock().unwrap().push("now".into());
                    1000
                }
            }),
            sleep: Arc::new({
                let calls = calls.clone();
                move |_| {
                    calls.lock().unwrap().push("sleep".into());
                    Box::pin(async {})
                }
            }),
        };
        wait_for_self_restart_parent_exit(handoff.as_ref(), &dependencies).await;
        assert_eq!(json!({"calls":*calls.lock().unwrap()}), expected, "old pid {old_pid:?}");
    }
}

#[test]
fn restart_handoff_clears_frozen_cli_model_overrides_only_when_consumed() {
    let mut source = oracle::TsOracle::spawn("self-restart-state-oracle.mts", "packages/coding-agent/src");
    for consumed in [false, true] {
        let args = json!({"provider":"old-provider","model":"old-model","thinking":"high"});
        let mut request = json!({"op":"apply","args":args});
        if consumed {
            request["handoff"] = json!({"sessionFile":"/s"});
        }
        let expected = source.request(&request);
        let mut args = Args {
            provider: Some("old-provider".into()),
            model: Some("old-model".into()),
            thinking: Some(pi_agent_core::harness::ThinkingLevel::High),
            ..Args::default()
        };
        let handoff = SelfRestartHandoff {
            session_file: JsString::from("/s"),
            prompt: None,
            old_pid: None,
        };
        apply_self_restart_request(&mut args, consumed.then_some(&handoff));
        let actual = json!({"provider":args.provider,"model":args.model,"thinking":args.thinking.map(|level|format!("{level:?}").to_lowercase())});
        let expected = json!({"provider":expected["args"].get("provider").unwrap_or(&Value::Null),"model":expected["args"].get("model").unwrap_or(&Value::Null),"thinking":expected["args"].get("thinking").unwrap_or(&Value::Null)});
        assert_eq!(actual, expected, "consumed {consumed}");
    }
}
