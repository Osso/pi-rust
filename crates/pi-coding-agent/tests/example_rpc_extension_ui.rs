#[path = "../examples/rpc_extension_ui.rs"]
mod client;
use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;
use pi_tui::{
    terminal::*,
    tui::{Component, Focusable},
};
use serde_json::{Value, json};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
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
            app.receive(&pi_js_compat::js_json::JsValue::from(event.clone()));
        }
        if let Some(input) = op["input"].as_str() {
            app.handle_input(&JsString::from(input));
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
    fn start(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
    fn stop(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.stops += 1;
        Ok(())
    }
    fn drain_input(&mut self, _: u64, _: u64) {}
    fn write(&mut self, s: &JsString) {
        self.written += &pi_js_compat::js_json::encode_node_utf8(s);
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
    fn set_title(&mut self, _: &JsString) {}
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
        prefix: vec![JsString::from(script.display().to_string())],
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
            prefix: vec![JsString::from(script.display().to_string())],
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
        prefix: vec![JsString::from(temp.path().display().to_string())],
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
static LIFECYCLE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ProcessCleanup {
    directory: PathBuf,
    subreaper: Option<rustix::process::Pid>,
}
impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        use rustix::process::{Pid, Signal, WaitOptions, kill_process, waitpid};
        let pids: Vec<_> = ["child.pid", "grandchild.pid"]
            .iter()
            .filter_map(|name| std::fs::read_to_string(self.directory.join(name)).ok())
            .map(|value| Pid::from_raw(value.parse().unwrap()).unwrap())
            .collect();
        for pid in &pids {
            match kill_process(*pid, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => panic!("kill fixture {pid:?}: {error}"),
            }
        }
        for pid in pids {
            match waitpid(Some(pid), WaitOptions::empty()) {
                Ok(_) | Err(rustix::io::Errno::CHILD) => {}
                Err(error) => panic!("reap fixture {pid:?}: {error}"),
            }
        }
        rustix::process::set_child_subreaper(self.subreaper).unwrap();
    }
}

async fn run_lifetime_fixture(descendant: bool) {
    let _serial = LIFECYCLE.lock().await;
    let temp = tempfile::tempdir().unwrap();
    let subreaper = rustix::process::child_subreaper().unwrap();
    rustix::process::set_child_subreaper(Some(rustix::process::getpid())).unwrap();
    let cleanup = ProcessCleanup {
        directory: temp.path().into(),
        subreaper,
    };
    let script = temp.path().join("lifetime.py");
    std::fs::write(&script, if descendant {
        "import os,pathlib,time\npathlib.Path('child.pid').write_text(str(os.getpid()))\ntime.sleep(.65)\npid=os.fork()\nif pid==0:\n pathlib.Path('grandchild.pid').write_text(str(os.getpid()))\n time.sleep(60)\nelse:\n while not pathlib.Path('grandchild.pid').exists(): time.sleep(.01)\n os._exit(0)\n"
    } else {
        "import os,pathlib,signal,time\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\npathlib.Path('child.pid').write_text(str(os.getpid()))\ntime.sleep(60)\n"
    }).unwrap();
    let terminal = Rc::new(RefCell::new(TerminalFixture::default()));
    let input = terminal.clone();
    let drive = async move {
        if !descendant {
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            input.borrow_mut().events.push(TerminalEvent::Input("\x1b".into()));
        }
    };
    let invocation = client::Invocation {
        program: "python3".into(),
        prefix: vec![JsString::from(script.display().to_string())],
        extension: "fixture".into(),
        cwd: temp.path().into(),
    };
    let (outcome, ()) = tokio::join!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client::run(invocation, terminal.clone())
        ),
        drive
    );
    let fixture_pids: Vec<_> = ["child.pid", "grandchild.pid"]
        .iter()
        .filter_map(|name| std::fs::read_to_string(temp.path().join(name)).ok())
        .map(|value| rustix::process::Pid::from_raw(value.parse().unwrap()).unwrap())
        .collect();
    // Cleanup runs before asserting, including the timeout/RED path.
    drop(cleanup);
    assert_eq!(fixture_pids.len(), if descendant { 2 } else { 1 });
    for pid in fixture_pids {
        assert_eq!(rustix::process::test_kill_process(pid), Err(rustix::io::Errno::SRCH));
    }
    let result = outcome
        .expect("client must not await process or inherited pipes")
        .unwrap();
    assert_eq!(result.code, 0);
    assert_eq!(
        result.stdout,
        if descendant { "Agent exited with code 0\n" } else { "" }
    );
    assert!(terminal.borrow().stops > 0);
}

#[tokio::test]
async fn idle_escape_returns_without_waiting_for_sigterm_ignoring_child() {
    run_lifetime_fixture(false).await;
}
#[tokio::test]
async fn child_exit_returns_while_grandchild_keeps_both_pipes_open() {
    run_lifetime_fixture(true).await;
}

#[tokio::test]
async fn invalid_utf8_stdout_line_does_not_hide_following_notification() {
    let mut ts = oracle::TsOracle::spawn(
        "example-rpc-extension-ui-oracle.ts",
        "packages/coding-agent/examples/rpc-extension-ui.ts",
    );
    let frame = json!({"type":"extension_ui_request","method":"notify","message":"after invalid bytes"});
    let mut raw = vec![255, 10];
    raw.extend(serde_json::to_vec(&frame).unwrap());
    raw.push(b'\n');
    let expected = ts.request(&json!({"ops":[{"raw":raw}],"widths":[80]}));
    assert!(
        expected["steps"][0]["renders"][0]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().contains("after invalid bytes"))
    );
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("invalid.py");
    std::fs::write(&script, "import sys,time,json\ntime.sleep(.6)\nsys.stdout.buffer.write(bytes([255,10]))\nsys.stdout.buffer.flush()\nprint(json.dumps({'type':'extension_ui_request','method':'notify','message':'after invalid bytes'}),flush=True)\ntime.sleep(.2)\n").unwrap();
    let terminal = Rc::new(RefCell::new(TerminalFixture::default()));
    let result = client::run(
        client::Invocation {
            program: "python3".into(),
            prefix: vec![JsString::from(script.display().to_string())],
            extension: "fixture".into(),
            cwd: temp.path().into(),
        },
        terminal.clone(),
    )
    .await
    .unwrap();
    assert_eq!(result.code, 0);
    assert!(terminal.borrow().written.contains("after invalid bytes"));
}

#[test]
fn utf16_clipping_preserves_split_surrogate_like_frozen() {
    let event =
        pi_js_compat::js_json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"🚀"}});
    let mut ts = oracle::TsOracle::spawn(
        "example-rpc-extension-ui-oracle.ts",
        "packages/coding-agent/examples/rpc-extension-ui.ts",
    );
    let expected: pi_js_compat::js_json::JsValue =
        ts.request(&pi_js_compat::js_json!({"ops":[{"event":event}],"widths":[19,20,21]}));
    let mut app = client::RpcExtensionUi::new();
    app.set_focused(true);
    app.receive(&event);
    for (i, width) in [19, 20, 21].into_iter().enumerate() {
        assert_eq!(
            pi_js_compat::js_json!(app.render(width)),
            expected["steps"][0]["renders"][i],
            "width {width}"
        );
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
