#[path = "../examples/extensions/rpc_demo.rs"]
mod example;
use crate::shared_oracle as oracle;

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

#[test]
fn all_rpc_dialogs_lifecycle_and_commands_match_protocol_frames() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let mut scripts = json!([
        {"command":{"id":"commands","type":"get_commands"}},
        {"command":{"id":"prefill","type":"prompt","message":"/rpc-prefill"}},
        {"command":{"id":"input","type":"prompt","message":"/rpc-input"},"response":{"value":"hello 🦀"}},
        {"command":{"id":"input-empty","type":"prompt","message":"/rpc-input"},"response":{"value":""}},
        {"command":{"id":"input-cancel","type":"prompt","message":"/rpc-input"},"response":{"cancelled":true}},
        {"command":{"id":"editor","type":"prompt","message":"/rpc-editor"},"response":{"value":"a\nb\n"}},
        {"command":{"id":"editor-empty","type":"prompt","message":"/rpc-editor"},"response":{"value":""}},
        {"command":{"id":"editor-cancel","type":"prompt","message":"/rpc-editor"},"response":{"cancelled":true}},
        {"command":{"id":"new-cancel","type":"new_session"},"response":{"confirmed":false}},
        {"command":{"id":"new-allow","type":"new_session"},"response":{"confirmed":true}}
    ]);
    let event_command = |id: &str, event: Value, response: Value| json!({"command":{"id":id,"type":"prompt","message":format!("/fixture-event {event}")},"response":response});
    let mut events = vec![
        event_command(
            "turn-start",
            json!({"type":"turn_start","turnIndex":0,"timestamp":1}),
            Value::Null,
        ),
        event_command(
            "turn-end",
            json!({"type":"turn_end","turnIndex":0,"message":{"role":"user","content":"fixture","timestamp":1},"toolResults":[]}),
            Value::Null,
        ),
        event_command(
            "turn-start-2",
            json!({"type":"turn_start","turnIndex":1,"timestamp":2}),
            Value::Null,
        ),
        event_command(
            "resume",
            json!({"type":"session_before_switch","reason":"resume"}),
            Value::Null,
        ),
        event_command(
            "new-no-ui",
            json!({"type":"session_before_switch","reason":"new","noUi":true}),
            Value::Null,
        ),
    ];
    for (index, (tool, command, response, no_ui)) in [
        ("read", "sudo rm -rf /fixture", Value::Null, false),
        ("bash", "echo safe", Value::Null, false),
        ("bash", "rm -rf /fixture", json!({"value":"Block"}), false),
        ("bash", "SUDO echo fixture", json!({"value":"Allow"}), false),
        ("bash", "rm --recursive /fixture", json!({"cancelled":true}), false),
        ("bash", "rm -r /fixture", Value::Null, true),
        ("bash", "éSUDO echo fixture", json!({"value":"Allow"}), false),
        ("bash", "presudo harmless", Value::Null, false),
        ("bash", "rm\u{feff}-r /fixture", json!({"value":"Block"}), false),
        ("bash", "rm\u{85}-r /fixture", json!({"value":"Block"}), false),
    ]
    .into_iter()
    .enumerate()
    {
        events.push(event_command(&format!("gate-{index}"),json!({"type":"tool_call","toolName":tool,"toolCallId":"fixture","input":{"command":command},"bypassPermissions":false,"noUi":no_ui}),response));
    }
    scripts.as_array_mut().unwrap().splice(1..1, events);
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-j-rpc-oracle.ts",
        "packages/coding-agent/examples/extensions/rpc-demo.ts",
    );
    let expected = oracle.request(&json!({"cwd":cwd,"scripts":scripts}));
    let mut child = Command::new(env!("CARGO_BIN_EXE_example-rpc-demo-harness"))
        .arg(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if sender.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let mut actual = Vec::<Value>::new();
    // Initial session_start UI requests arrive when RPC binds the extension runner.
    loop {
        let line = receiver.recv_timeout(Duration::from_secs(20)).unwrap();
        let frame: Value = serde_json::from_str(&line).unwrap();
        let ready = frame["method"] == "setStatus";
        actual.push(frame);
        if ready {
            break;
        }
    }
    for step in scripts.as_array().unwrap() {
        writeln!(stdin, "{}", step["command"]).unwrap();
        stdin.flush().unwrap();
        loop {
            let line = receiver.recv_timeout(Duration::from_secs(20)).unwrap();
            let frame: Value = serde_json::from_str(&line).unwrap();
            let done = frame["type"] == "response" && frame["id"] == step["command"]["id"];
            if frame["type"] == "extension_ui_request"
                && ["input", "editor", "confirm", "select"]
                    .iter()
                    .any(|m| frame["method"] == *m)
            {
                let mut response = step["response"].clone();
                response["type"] = json!("extension_ui_response");
                response["id"] = frame["id"].clone();
                writeln!(stdin, "{response}").unwrap();
                stdin.flush().unwrap();
            }
            actual.push(frame);
            if done {
                break;
            }
        }
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let normalize = |frames: &Value| {
        let mut value = frames.clone();
        for frame in value.as_array_mut().unwrap() {
            if frame["type"] == "extension_ui_request" {
                frame["id"] = json!("<ui-id>");
            }
        }
        value
    };
    let actual = normalize(&json!(actual));
    let expected = normalize(&expected);
    for (index, (actual, expected)) in actual
        .as_array()
        .unwrap()
        .iter()
        .zip(expected.as_array().unwrap())
        .enumerate()
    {
        assert_eq!(actual, expected, "RPC frame {index}");
    }
    assert_eq!(actual, expected, "complete RPC transcript");
    // Compile the actual embeddable factory, not a library re-export.
    let _: fn(
        &pi_coding_agent::core::extensions::ExtensionApi,
    ) -> Result<(), pi_coding_agent::core::extensions::HandlerError> = example::rpc_demo_extension;
}
