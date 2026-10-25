//! Resident/client process boundaries, sandbox-owned files/processes and an executable fake SSH only.
use crate::shared_oracle as oracle;
use pi_coding_agent::core::host_relay_resident::*;
use pi_coding_agent::core::runtime_process::{is_process_identity_alive, read_process_identity};
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("fixture deadline exceeded")
}
fn terminate(pid: u32) {
    let pid = rustix::process::Pid::from_raw(pid as i32).unwrap();
    let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
}
struct Fixture {
    temp: tempfile::TempDir,
    exe: PathBuf,
    db: String,
}
impl Fixture {
    fn new(configured: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("pi");
        fs::copy(env!("CARGO_BIN_EXE_host-relay-harness"), &exe).unwrap();
        let db = temp
            .path()
            .join("control.sqlite")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        if configured {
            fs::write(
                temp.path().join("settings.json"),
                json!({"relay":{"peer":"fake-peer","remoteCommand":"fixture relay serve"}}).to_string(),
            )
            .unwrap();
        }
        fs::write(
            temp.path().join("ssh"),
            r#"#!/usr/bin/python3
import json, os, sys
from pathlib import Path
root=Path(sys.argv[0]).parent
with (root/'starts').open('a') as file: file.write(str(os.getpid())+'\n')
(root/'argv.json').write_text(json.dumps(sys.argv[1:]))
(root/'peer-pid').write_text(str(os.getpid()))
if (root/'fail-first').exists():
 (root/'fail-first').unlink()
 sys.stderr.write('fixture peer unavailable\n')
 sys.exit(255)
print(json.dumps({'type':'hello','host':'fake-peer'}),flush=True)
for line in sys.stdin:
 frame=json.loads(line)
 if frame['type']=='mailbox':
  with (root/'received').open('a') as file: file.write(json.dumps(frame)+'\n')
  print(json.dumps({'type':'ack','kind':'mailbox','id':frame['seq']}),flush=True)
 elif frame['type']=='channel': print(json.dumps({'type':'ack','kind':'channel','id':frame['originId']}),flush=True)
"#,
        )
        .unwrap();
        fs::set_permissions(temp.path().join("ssh"), fs::Permissions::from_mode(0o755)).unwrap();
        Self { temp, exe, db }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.env("HOME", self.temp.path())
            .env("PI_CODING_AGENT_DIR", self.temp.path())
            .env("PI_CODING_AGENT_STATE_DIR", self.temp.path())
            .env("PI_OFFLINE", "1")
            .env(
                "PATH",
                format!("{}:{}", self.temp.path().display(), std::env::var("PATH").unwrap()),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }
    fn spawn(&self, args: &[&str]) -> OwnedChild {
        OwnedChild(self.command().args(args).spawn().unwrap())
    }
    fn ensure(&self) {
        let out = self.command().args(["ensure", &self.db]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    fn resident(&self) -> Option<RelayIdentity> {
        read_relay_identity(&self.db).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(Some(identity)) = read_relay_identity(&self.db) {
            terminate(identity.pid);
            let deadline = Instant::now() + Duration::from_secs(2);
            while read_relay_identity(&self.db).ok().flatten().is_some() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if read_relay_identity(&self.db).ok().flatten().is_some() {
                let _ = rustix::process::kill_process(
                    rustix::process::Pid::from_raw(identity.pid as i32).unwrap(),
                    rustix::process::Signal::KILL,
                );
            }
        }
    }
}
fn queue(db: &str) {
    let input:EnqueueStoredRuntimeMailboxMessageInput=serde_json::from_value(json!({"kind":"message","recipientHost":"fake-peer","recipient":{"sessionId":"remote","agentId":null},"sender":{"sessionId":"local","agentId":null},"storeRef":{"sessionPath":"/fixture/s","messageId":"startup"},"message":{"id":"startup","kind":"message","status":"pending","fromAgentId":"main","toAgentId":"main","body":"outage-safe"}})).unwrap();
    enqueue_stored_runtime_mailbox_message(db, &input).unwrap();
}
#[test]
fn client_reconnects_after_failure_preserves_queue_and_stops_ssh_on_term() {
    let f = Fixture::new(true);
    fs::write(f.temp.path().join("fail-first"), "").unwrap();
    queue(&f.db);
    let mut child = f.spawn(&["relay", "connect"]);
    wait_until(|| f.temp.path().join("received").exists());
    wait_until(|| list_runtime_mailbox_messages(&f.db).unwrap().is_empty());
    let starts = fs::read_to_string(f.temp.path().join("starts")).unwrap();
    assert_eq!(starts.lines().count(), 2);
    let argv: Value = serde_json::from_str(&fs::read_to_string(f.temp.path().join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        argv,
        json!([
            "-o",
            "BatchMode=yes",
            "-o",
            "ServerAliveInterval=15",
            "fake-peer",
            "fixture relay serve"
        ])
    );
    let peer: u32 = fs::read_to_string(f.temp.path().join("peer-pid"))
        .unwrap()
        .parse()
        .unwrap();
    let identity = read_process_identity(peer).unwrap();
    terminate(child.0.id());
    wait_until(|| child.0.try_wait().unwrap().is_some());
    assert!(!is_process_identity_alive(&identity));
    assert!(!Path::new(&format!("{}.relay.json", f.db)).exists());
}
#[test]
fn concurrent_startup_creates_one_resident_reuses_version_and_replaces_old_version() {
    let f = Fixture::new(true);
    queue(&f.db);
    let mut children: Vec<_> = (0..3).map(|_| f.spawn(&["ensure", &f.db])).collect();
    for child in &mut children {
        assert!(child.0.wait().unwrap().success());
    }
    let first = f.resident().unwrap();
    wait_until(|| f.temp.path().join("received").exists());
    assert_eq!(
        fs::read_to_string(f.temp.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    f.ensure();
    assert_eq!(f.resident().unwrap().pid, first.pid);
    let process = read_process_identity(first.pid).unwrap();
    let mut stale = first;
    stale.version = "0.0.0".into();
    write_relay_identity(&f.db, &stale).unwrap();
    f.ensure();
    assert_ne!(f.resident().unwrap().pid, stale.pid);
    assert!(!is_process_identity_alive(&process));
    let new = f.resident().unwrap();
    terminate(new.pid);
    wait_until(|| f.resident().is_none());
    assert!(!Path::new(&format!("{}.relay.json", f.db)).exists());
}
#[tokio::test]
async fn startup_lock_is_shared_and_awaiting_it_does_not_block_executor() {
    let f = Fixture::new(true);
    let lock = pi_coding_agent::utils::lockfile::acquire(
        Path::new(&f.db),
        &pi_coding_agent::utils::lockfile::LockOptions {
            lockfile_path: Some(PathBuf::from(format!("{}.relay-start.lock", f.db))),
            realpath: false,
            stale_ms: 5000,
            update_ms: Some(1000),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut child = f.spawn(&["ensure", &f.db]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(child.0.try_wait().unwrap().is_none());
    assert!(f.resident().is_none());
    lock.release().unwrap();
    tokio::task::spawn_blocking(move || {
        assert!(child.0.wait().unwrap().success());
        f.resident().unwrap();
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn absent_settings_spawn_nothing_and_invalid_identity_refuses_unverified_process() {
    let f = Fixture::new(false);
    let output = f.command().args(["relay", "connect"]).output().unwrap();
    assert!(output.status.success());
    assert!(f.resident().is_none());
    assert!(!f.temp.path().join("starts").exists());
    ensure_relay_running(&JsString::from(&f.db), None).await.unwrap();
    assert!(!Path::new(&format!("{}.relay.log", f.db)).exists());
    let identity = read_process_identity(std::process::id()).unwrap();
    write_relay_identity(
        &f.db,
        &RelayIdentity {
            pid: identity.pid,
            start_time_ticks: identity.start_time_ticks,
            version: "old".into(),
        },
    )
    .unwrap();
    let settings = pi_coding_agent::core::host_relay_client::RelaySettings {
        peer: "fake-peer".into(),
        remote_command: None,
    };
    let error = ensure_relay_running(&JsString::from(&f.db), Some(&settings))
        .await
        .unwrap_err();
    assert!(error.starts_with("Refusing to terminate unverified relay process"));
    fs::remove_file(format!("{}.relay.json", f.db)).unwrap();
    fs::write(
        format!("{}.relay.json", f.db),
        r#"{"pid":"wrong","startTimeTicks":1,"version":"x"}"#,
    )
    .unwrap();
    assert!(
        read_relay_identity(&f.db)
            .unwrap_err()
            .contains("Invalid relay identity")
    );
    fs::remove_file(format!("{}.relay.json", f.db)).unwrap();
}
#[test]
fn ssh_peer_validation_and_backoff_boundaries() {
    use pi_coding_agent::core::host_relay_client::*;
    for peer in ["", " \t", "-oProxyCommand=x"] {
        assert!(validate_peer(&peer.into()).is_err());
    }
    validate_peer(&" peer-alias ".into()).unwrap();
    for (failures, min, max) in [
        (0, 500, 1000),
        (1, 1000, 2000),
        (4, 8000, 16000),
        (5, 15000, 30000),
        (100, 15000, 30000),
    ] {
        assert_eq!(reconnect_delay_ms(failures, 0.0), min);
        assert_eq!(reconnect_delay_ms(failures, 1.0), max);
    }
    assert_eq!(
        ssh_arguments(&RelaySettings {
            peer: "p".into(),
            remote_command: None
        })
        .last()
        .unwrap(),
        "pi relay serve"
    );
}

#[test]
fn relay_settings_keep_lone_surrogates_until_the_ssh_argv_boundary() {
    use pi_coding_agent::core::host_relay_client::ssh_arguments;
    use pi_js_compat::js_json::{JsValue, parse_lossless};

    let JsValue::Object(settings) =
        parse_lossless(r#"{"relay":{"peer":"h\ud800","remoteCommand":"pi \udc00"}}"#).unwrap()
    else {
        panic!("settings object");
    };
    let relay = relay_settings_from_global(&settings).unwrap().unwrap();
    assert_eq!(relay.peer.units(), [0x68, 0xd800]);
    let arguments = ssh_arguments(&relay);
    assert_eq!(arguments[4].units(), [0x68, 0xd800]);
    assert_eq!(arguments[5].units(), [0x70, 0x69, 0x20, 0xdc00]);

    let JsValue::Object(invalid) = parse_lossless(r#"{"relay":{"peer":7}}"#).unwrap() else {
        panic!("settings object");
    };
    assert_eq!(
        relay_settings_from_global(&invalid).unwrap_err(),
        "relay.peer must be a string"
    );
}

#[test]
fn frozen_client_runs_same_fake_outage_reconnect_and_ack_on_rust_queue() {
    let f = Fixture::new(true);
    fs::write(f.temp.path().join("fail-first"), "").unwrap();
    queue(&f.db);
    let path = format!("{}:{}", f.temp.path().display(), std::env::var("PATH").unwrap());
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "host-relay-oracle.ts",
        "packages/coding-agent/src",
        &[("PATH", path.into())],
    );
    let result=ts.request(&json!({"op":"client","path":f.db,"settings":{"peer":"fake-peer","remoteCommand":"fixture relay serve"},"readyPath":f.temp.path().join("received")}));
    assert_eq!(result["ok"], true, "{result}");
    let logs: Vec<String> = serde_json::from_str(result["wire"].as_str().unwrap()).unwrap();
    assert_eq!(logs, vec!["[relay:fake-peer] ssh exited 255: fixture peer unavailable"]);
    assert_eq!(
        fs::read_to_string(f.temp.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(list_runtime_mailbox_messages(&f.db).unwrap().is_empty());
    let pid: u32 = fs::read_to_string(f.temp.path().join("peer-pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(read_process_identity(pid).is_err());
    let argv: Value = serde_json::from_str(&fs::read_to_string(f.temp.path().join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        argv,
        json!([
            "-o",
            "BatchMode=yes",
            "-o",
            "ServerAliveInterval=15",
            "fake-peer",
            "fixture relay serve"
        ])
    );
}

#[tokio::test]
async fn resident_identity_bytes_and_reuse_refusal_errors_match_frozen_bidirectionally() {
    let t = tempfile::tempdir().unwrap();
    let path = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("host-relay-oracle.ts", "packages/coding-agent/src");
    let result = ts.request(&json!({"op":"writeIdentity","path":path}));
    assert_eq!(result["ok"], true, "{result}");
    let ts_wire: String = serde_json::from_str(result["wire"].as_str().unwrap()).unwrap();
    let identity = read_relay_identity(&path).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&ts_wire).unwrap()["version"],
        serde_json::to_value(&identity.version).unwrap()
    );
    write_relay_identity(&path, &identity).unwrap();
    assert_eq!(fs::read_to_string(format!("{path}.relay.json")).unwrap(), ts_wire);
    let result = ts.request(&json!({"op":"ensure","path":path,"settings":{"peer":"never-invoked"}}));
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(
        serde_json::from_str::<String>(result["wire"].as_str().unwrap()).unwrap(),
        ts_wire
    );
    let mut old = identity;
    old.version = "0.0.0".into();
    write_relay_identity(&path, &old).unwrap();
    let settings = pi_coding_agent::core::host_relay_client::RelaySettings {
        peer: "never-invoked".into(),
        remote_command: None,
    };
    let rust = ensure_relay_running(&JsString::from(&path), Some(&settings))
        .await
        .unwrap_err();
    let result = ts.request(&json!({"op":"ensure","path":path,"settings":{"peer":"never-invoked"}}));
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"], rust);
    for wire in [
        "{",
        r#"{"pid":"wrong","startTimeTicks":1,"version":"x"}"#,
        r#"{"pid":1,"version":"x"}"#,
    ] {
        fs::write(format!("{path}.relay.json"), wire).unwrap();
        let rust = read_relay_identity(&path).unwrap_err();
        let result = ts.request(&json!({"op":"ensure","path":path,"settings":{"peer":"never-invoked"}}));
        assert_eq!(result["ok"], false);
        assert_eq!(result["error"], rust);
    }
    assert!(!Path::new(&format!("{path}.relay-start.lock")).exists());
}

#[test]
fn spawn_failure_is_logged_like_frozen_without_executing_any_ssh() {
    use std::io::Read;
    let f = Fixture::new(true);
    fs::set_permissions(f.temp.path().join("ssh"), fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink("/usr/bin/node", f.temp.path().join("node")).unwrap();
    let mut cmd = f.command();
    cmd.env("PATH", f.temp.path()).stderr(Stdio::piped());
    let mut child = OwnedChild(cmd.args(["relay", "connect"]).spawn().unwrap());
    std::thread::sleep(Duration::from_millis(100));
    terminate(child.0.id());
    assert!(child.0.wait().unwrap().success());
    let mut log = String::new();
    child.0.stderr.take().unwrap().read_to_string(&mut log).unwrap();
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "host-relay-oracle.ts",
        "packages/coding-agent/src",
        &[("PATH", f.temp.path().as_os_str().to_owned())],
    );
    let response=ts.request(&json!({"op":"client","path":f.db,"settings":{"peer":"fake-peer","remoteCommand":"fixture relay serve"},"stopAfterMs":100}));
    assert_eq!(response["ok"], true, "{response}");
    let expected: Vec<String> = serde_json::from_str(response["wire"].as_str().unwrap()).unwrap();
    assert_eq!(log.lines().collect::<Vec<_>>(), expected);
    assert_eq!(expected, vec!["[relay:fake-peer] spawn ssh EACCES"]);
    assert!(!f.temp.path().join("starts").exists());
}
