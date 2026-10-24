//! Real stdin/stdout lifecycle differential inside the mandatory sandbox's PTYs.
use pi_js_compat::js_json::JsString;
use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

use pi_tui::terminal::TerminalEvent;
use pi_tui::{ProcessTerminal, Terminal};
use serde_json::{Value, json};

const DRIVER: &str = r#"
import fcntl, json, os, select, struct, subprocess, sys, signal, termios, time
command, case = json.loads(sys.argv[1]), json.loads(sys.argv[2])
pty = case['kind'] != 'pipe'
master = slave = None
if pty:
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 5, 20, 0, 0))
    before = termios.tcgetattr(slave)
def attach():
    os.setsid()
    if pty: fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
env = dict(os.environ, Q1_STREAM_CHILD='1', Q1_STREAM_CYCLES=str(case['cycles']))
child = subprocess.Popen(command, stdin=slave if pty else subprocess.PIPE, stdout=slave if pty else subprocess.PIPE, stderr=subprocess.PIPE, env=env, preexec_fn=attach)
def line():
    if not select.select([child.stderr], [], [], 8)[0]: raise RuntimeError('terminal fixture timed out')
    result = child.stderr.readline()
    if not result: raise RuntimeError('fixture exited: ' + str(child.poll()))
    return result.decode().rstrip('\n')
def send(data):
    if pty: os.write(master, data)
    else: child.stdin.write(data); child.stdin.flush()
results = []
try:
    for cycle in range(case['cycles']):
        ready = line()
        if ready != 'READY': raise RuntimeError('unexpected fixture output: ' + ready)
        if case['kind'] == 'kitty':
            send(b'\x1b[?7'); time.sleep(.025); send(b'u')
        else: send(b'\x1b[?62;4;52c')
        time.sleep(.025)
        if case['utf8']:
            send(b'\xc3'); time.sleep(.025); send(b'\xa9')
        else: send(b'a')
        send('\x1b[200~paste🙂\x1b[201~'.encode())
        time.sleep(.025)
        if pty:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 7+cycle, 30+cycle, 0, 0))
            time.sleep(.04)
        send(b'q')
        draining = line()
        if draining != 'DRAINING': raise RuntimeError('drain handshake missing: ' + draining)
        time.sleep(.05); send(b'\x1b[?0u')
        result = line()
        if not result.startswith('RESULT '): raise RuntimeError('unexpected fixture result: ' + result)
        results.append(json.loads(result[7:]))
    child.wait(timeout=8)
    if pty:
        output = bytearray()
        while select.select([master], [], [], .05)[0]: output.extend(os.read(master, 65536))
        restored = termios.tcgetattr(slave) == before
    else:
        output = child.stdout.read(); restored = None
    print(json.dumps(dict(results=results, writes=bytes(output).decode(), restored=restored, exit=child.returncode)))
except Exception as error:
    try: os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError: pass
    child.wait()
    print(json.dumps(dict(error=str(error), stderr=child.stderr.read().decode(errors='replace'))))
finally:
    if master is not None: os.close(master)
    if slave is not None: os.close(slave)
"#;

fn collect_inputs(terminal: &mut ProcessTerminal, inputs: &mut Vec<JsString>, sizes: &mut Vec<[usize; 2]>) -> bool {
    let mut done = false;
    for event in terminal.poll_events() {
        match event {
            TerminalEvent::Input(data) => {
                done |= data == "q";
                inputs.push(data);
            }
            TerminalEvent::Resize => sizes.push([terminal.columns(), terminal.rows()]),
        }
    }
    done
}

fn run_child() {
    let cycles: usize = std::env::var("Q1_STREAM_CYCLES").unwrap().parse().unwrap();
    for _ in 0..cycles {
        let mut terminal = ProcessTerminal::new();
        let mut inputs = Vec::new();
        let mut sizes = Vec::new();
        terminal.start().unwrap();
        terminal.set_progress(true);
        let initial = Instant::now();
        while initial.elapsed() < Duration::from_millis(25) {
            collect_inputs(&mut terminal, &mut inputs, &mut sizes);
            std::thread::sleep(Duration::from_millis(1));
        }
        eprintln!("READY");
        std::io::stderr().flush().unwrap();
        while !collect_inputs(&mut terminal, &mut inputs, &mut sizes) {
            std::thread::sleep(Duration::from_millis(1));
        }
        let kitty = terminal.kitty_protocol_active();
        let modify = terminal.modify_other_keys_active();
        let drain_start = Instant::now();
        eprintln!("DRAINING");
        terminal.drain_input(1000, 300);
        let waited_for_late_data = drain_start.elapsed() >= Duration::from_millis(325);
        terminal.stop().unwrap();
        eprintln!(
            "RESULT {}",
            json!({"inputs":inputs,"sizes":sizes,"kitty":kitty,"modify":modify,"waitedForLateData": waited_for_late_data})
        );
        std::thread::sleep(Duration::from_millis(40));
    }
}

fn drive(command: Value, case: &Value) -> Value {
    let output = Command::new("python3")
        .args(["-c", DRIVER, &command.to_string(), &case.to_string()])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("PTY driver JSON")
}

fn main() {
    if std::env::var_os("Q1_STREAM_CHILD").is_some() {
        run_child();
        return;
    }
    let lock: Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    let source = lock["worktree"].as_str().unwrap();
    let head = Command::new("git")
        .args(["-C", source, "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(head.stdout).unwrap().trim(),
        lock["commit"].as_str().unwrap()
    );
    let oracle = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/terminal-stream-fixture.ts");
    let reference = json!([
        format!("{source}/node_modules/.bin/tsx"),
        oracle,
        format!("{source}/packages/tui/src/terminal.ts")
    ]);
    let rust = json!([std::env::current_exe().unwrap()]);
    for case in [
        json!({"kind":"kitty","cycles":1,"utf8":true}),
        json!({"kind":"da","cycles":2,"utf8":false}),
        json!({"kind":"pipe","cycles":1,"utf8":true}),
    ] {
        let expected = drive(reference.clone(), &case);
        assert!(expected.get("error").is_none(), "frozen fixture: {expected}");
        let actual = drive(rust.clone(), &case);
        assert_eq!(actual, expected, "case {case}");
        println!("passed real stream case {case}");
    }
}
