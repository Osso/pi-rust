#[path = "../examples/rpc_extension_ui.rs"]
mod client;
#[path = "support/oracle.rs"]
mod oracle;
use pi_tui::{
    terminal::*,
    tui::{Component, Focusable},
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn operations() -> Vec<Value> {
    vec![
        json!({"input":" hello "}),
        json!({"input":"\r"}),
        json!({"event":{"type":"agent_start"}}),
        json!({"tick":true}),
        json!({"event":{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Hello\nworld"}}}),
        json!({"event":{"type":"extension_ui_request","id":"s","method":"select","title":"Pick","options":["one","two"]}}),
        json!({"input":"\x1b[B"}),
        json!({"input":"\r"}),
        json!({"event":{"type":"extension_ui_request","id":"c","method":"confirm","title":"Safe?","message":"Trust repo"}}),
        json!({"input":"\x1b"}),
        json!({"event":{"type":"extension_ui_request","id":"i","method":"input","title":"Name","placeholder":"enter"}}),
        json!({"input":" Ada "}),
        json!({"input":"\r"}),
        json!({"event":{"type":"extension_ui_request","id":"e","method":"editor","prefill":"first\nsecond"}}),
        json!({"input":"\r"}),
        json!({"event":{"type":"extension_ui_request","id":"n","method":"notify","message":"warn","notifyType":"warning"}}),
        json!({"event":{"type":"extension_ui_request","id":"x","method":"setStatus","statusKey":"test"}}),
        json!({"event":{"type":"extension_ui_request","id":"x","method":"setWidget","widgetKey":"fixture","widgetLines":["line 1","line 2"]}}),
        json!({"event":{"type":"tool_execution_start","toolName":"read"}}),
        json!({"event":{"type":"tool_execution_end","result":{"text":"abc"}}}),
        json!({"event":{"type":"response","success":false,"command":"bad","error":"no"}}),
        json!({"input":"\x1b"}),
        json!({"event":{"type":"agent_end"}}),
        json!({"event":{"type":"extension_ui_request","method":"set_editor_text","text":"/select"}}),
        json!({"input":"\r"}),
        json!({"input":"\r"}),
        json!({"event":{"type":"extension_ui_request","method":"set_editor_text","text":"/confirm"}}),
        json!({"input":"\r"}),
        json!({"input":"\x1b"}),
        json!({"event":{"type":"extension_ui_request","method":"set_editor_text","text":"/input"}}),
        json!({"input":"\r"}),
        json!({"input":"\x1b"}),
        json!({"event":{"type":"extension_ui_request","method":"set_editor_text","text":"/editor"}}),
        json!({"input":"\r"}),
        json!({"input":"\r"}),
        json!({"input":"\x04"}),
        json!({"input":"\x1b"}),
    ]
}
#[test]
fn rpc_protocol_dialogs_components_and_local_commands_match_pinned_program() {
    let mut ts = oracle::TsOracle::spawn(
        "example-rpc-extension-ui-oracle.ts",
        "packages/coding-agent/examples/rpc-extension-ui.ts",
    );
    let ops = operations();
    let expected = ts.request(&json!({"ops":ops,"widths":[20,100]}));
    let mut app = client::RpcExtensionUi::new();
    app.set_focused(true);
    for (i, op) in ops.iter().enumerate() {
        if let Some(event) = op.get("event") {
            app.receive(event);
        }
        if let Some(input) = op["input"].as_str() {
            app.handle_input(input);
        }
        if op["tick"] == true {
            app.tick();
        }
        let renders = [20, 100].map(|w| app.render(w));
        let commands = app.take_commands();
        assert_eq!(
            json!({"renders":renders,"commands":commands,"exit":app.exit_requested()}),
            expected["steps"][i],
            "operation {i}: {op}"
        );
    }
}
#[derive(Default)]
struct TerminalFixture {
    written: String,
    events: Vec<TerminalEvent>,
    stops: usize,
}
impl Terminal for TerminalFixture {
    fn start(&mut self) -> Result<(), TerminalRawModeError> {
        Ok(())
    }
    fn stop(&mut self) {
        self.stops += 1;
    }
    fn drain_input(&mut self, _: u64, _: u64) {}
    fn write(&mut self, s: &str) {
        self.written += s;
    }
    fn columns(&self) -> usize {
        80
    }
    fn rows(&self) -> usize {
        40
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _: isize) {}
    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _: &str) {}
    fn set_progress(&mut self, _: bool) {}
    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.events)
    }
}
#[tokio::test]
async fn real_fixture_process_and_native_tui_exchange_rpc_and_clean_up() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("rpc.py");
    std::fs::write(&script,"import sys,json,time\ntime.sleep(.6)\nprint(json.dumps({'type':'extension_ui_request','id':'ask','method':'input','title':'Fixture name'}),flush=True)\nfor line in sys.stdin:\n r=json.loads(line)\n if r.get('type')=='extension_ui_response':\n  print(json.dumps({'type':'extension_ui_request','method':'notify','message':'answer '+r.get('value','cancelled')}),flush=True)\n  break\nsys.stderr.write('fixture stderr')\n").unwrap();
    let terminal = Rc::new(RefCell::new(TerminalFixture::default()));
    let input = terminal.clone();
    let driver = async move {
        tokio::time::sleep(std::time::Duration::from_millis(850)).await;
        input
            .borrow_mut()
            .events
            .extend([TerminalEvent::Input("Ada".into()), TerminalEvent::Input("\r".into())]);
    };
    let invocation = client::Invocation {
        program: "python3".into(),
        prefix: vec![script.display().to_string()],
        extension: "<example:rpc-demo>".into(),
        cwd: temp.path().into(),
    };
    let (result, ()) = tokio::join!(client::run(invocation, terminal.clone()), driver);
    let result = result.unwrap();
    assert_eq!(result.code, 0);
    assert_eq!(result.stdout, "Agent exited with code 0\n");
    assert_eq!(result.stderr, "fixture stderr\n");
    assert!(terminal.borrow().written.contains("answer Ada"));
    assert!(terminal.borrow().stops > 0);
    // The client's 500ms early-exit boundary is tested separately from normal exit.
    let script = temp.path().join("early.py");
    std::fs::write(&script, "import sys\nsys.stderr.write('early stderr')\nsys.exit(7)\n").unwrap();
    let result = client::run(
        client::Invocation {
            program: "python3".into(),
            prefix: vec![script.display().to_string()],
            extension: "fixture".into(),
            cwd: temp.path().into(),
        },
        Rc::new(RefCell::new(TerminalFixture::default())),
    )
    .await
    .unwrap();
    assert_eq!(result.code, 1);
    assert_eq!(result.stderr, "Agent exited immediately. Stderr:\nearly stderr\n");
}

#[tokio::test]
async fn native_rpc_demo_extension_round_trip_uses_real_agent_session_harness() {
    let temp = tempfile::tempdir().unwrap();
    let terminal = Rc::new(RefCell::new(TerminalFixture::default()));
    let input = terminal.clone();
    let drive = async move {
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        input.borrow_mut().events.extend([
            TerminalEvent::Input("/rpc-input".into()),
            TerminalEvent::Input("\r".into()),
        ]);
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        input
            .borrow_mut()
            .events
            .extend([TerminalEvent::Input("Ada".into()), TerminalEvent::Input("\r".into())]);
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        input.borrow_mut().events.push(TerminalEvent::Input("\x1b".into()));
    };
    let invocation = client::Invocation {
        program: env!("CARGO_BIN_EXE_example-rpc-demo-harness").into(),
        prefix: vec![temp.path().display().to_string()],
        extension: "rpc-demo".into(),
        cwd: temp.path().into(),
    };
    let (result, ()) = tokio::join!(client::run(invocation, terminal.clone()), drive);
    let result = result.unwrap();
    assert_eq!(result.code, 0);
    assert!(
        terminal.borrow().written.contains("You entered: Ada"),
        "{}",
        terminal.borrow().written
    );
    assert!(terminal.borrow().stops > 0);
}
#[test]
fn utf16_clipping_projects_split_surrogate_at_string_boundary() {
    let mut app = client::RpcExtensionUi::new();
    app.receive(&json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"🚀"}}));
    // Raw source slices include ANSI bytes; 20 units retain half of the rocket pair.
    assert!(app.render(20).iter().any(|line| line.contains('\u{fffd}')));
}
