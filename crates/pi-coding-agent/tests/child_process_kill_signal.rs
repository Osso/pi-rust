use pi_coding_agent::utils::child_process::{CaptureStop, capture_child_process};
use std::process::Stdio;
use std::time::Duration;

const WRITER: &str = r#"import os,sys,signal,json,errno,time
def terminated(_signal,_frame):
    closed=False
    until=time.monotonic()+1
    while time.monotonic()<until:
        try: os.write(1,b'trap')
        except OSError as error:
            closed=error.errno==errno.EPIPE
            break
    with open(sys.argv[1],'w') as output: json.dump({'signal':'SIGTERM','stdoutClosed':closed},output)
    sys.exit(0)
signal.signal(signal.SIGTERM,terminated)
while True:
    try: os.write(1,b'x'*65536)
    except BrokenPipeError: time.sleep(0.01)
"#;

struct PrivateGroup(rustix::process::Pid);
impl Drop for PrivateGroup {
    fn drop(&mut self) {
        match rustix::process::kill_process_group(self.0, rustix::process::Signal::KILL) {
            Ok(()) | Err(rustix::io::Errno::SRCH) => {}
            Err(error) => panic!("clean private process group: {error}"),
        }
    }
}
fn group(child: &tokio::process::Child) -> PrivateGroup {
    PrivateGroup(rustix::process::Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()).unwrap())
}

#[tokio::test]
async fn overflow_uses_requested_sigterm_and_closes_pipes_before_the_child_trap() {
    let root = tempfile::tempdir().unwrap();
    let source_marker = root.path().join("source.json");
    let native_marker = root.path().join("native.json");
    let javascript = "require('node:child_process').execFile('/usr/bin/python3',['-c',process.argv[1],process.argv[2]],{maxBuffer:1024},error=>{if(!error)process.exit(7);process.stdout.write(JSON.stringify({code:error.code}));});";
    let source = tokio::process::Command::new("node")
        .args(["-e", javascript, WRITER])
        .arg(&source_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let _source_group = group(&source);
    let output = tokio::time::timeout(Duration::from_secs(5), source.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let source_error: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source_error["code"], "ERR_CHILD_PROCESS_STDIO_MAXBUFFER");
    let expected: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(source_marker).unwrap()).unwrap();
    assert_eq!(expected, serde_json::json!({"signal":"SIGTERM","stdoutClosed":true}));
    let mut child = tokio::process::Command::new("/usr/bin/python3")
        .args(["-c", WRITER])
        .arg(&native_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let _native_group = group(&child);
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        capture_child_process(&mut child, Some(1024), None, rustix::process::Signal::TERM),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output.stopped, Some(CaptureStop::StdoutLimit));
    let actual: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(native_marker).expect("requested TERM trap must run")).unwrap();
    assert_eq!(actual, expected);
    let kill_marker = root.path().join("kill.json");
    let mut child = tokio::process::Command::new("/usr/bin/python3")
        .args(["-c", WRITER])
        .arg(&kill_marker)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let _kill_group = group(&child);
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        capture_child_process(&mut child, Some(1024), None, rustix::process::Signal::KILL),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output.stopped, Some(CaptureStop::StdoutLimit));
    assert!(
        !kill_marker.exists(),
        "existing KILL callers must not run TERM handlers"
    );
}
