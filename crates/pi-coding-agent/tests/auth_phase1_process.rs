//! Actual healthy process/heartbeat protocol proof; all paths are private sandbox fixtures.
use pi_coding_agent::core::auth_storage::{AuthStorageBackend, FileAuthStorageBackend};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    time::{Duration, SystemTime},
};

struct Peer {
    child: Child,
    stdout: BufReader<ChildStdout>,
}
impl Peer {
    fn spawn(mut command: Command) -> Self {
        use std::os::unix::process::CommandExt;
        let mut child = command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdout }
    }
    fn line(&mut self) -> String {
        let mut s = String::new();
        assert_ne!(self.stdout.read_line(&mut s).unwrap(), 0);
        s.trim().to_owned()
    }
    fn finish(&mut self) {
        assert!(self.child.wait().unwrap().success());
    }
}
impl Peer {
    fn kill(&mut self) {
        use rustix::process::{Pid, Signal, kill_process_group};
        if let Some(pid) = Pid::from_raw(self.child.id() as i32) {
            let _ = kill_process_group(pid, Signal::KILL);
        }
        let _ = self.child.wait();
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.kill();
    }
}
fn source() -> PathBuf {
    let v: Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    PathBuf::from(v["worktree"].as_str().unwrap())
}
fn ts(mode: &str, file: &Path, ms: &str) -> Peer {
    let root = source();
    let mut c = Command::new(root.join("node_modules/.bin/tsx"));
    c.arg("--tsconfig")
        .arg(root.join("tsconfig.json"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/auth-phase1-peer.ts"))
        .arg(root)
        .arg(mode)
        .arg(file)
        .arg(ms);
    Peer::spawn(c)
}
fn fixture(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "auth-process-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    let p = d.join("auth.json");
    fs::write(&p, "{}").unwrap();
    p
}
#[test]
fn healthy_ts_lock_excludes_rust_sync_at_three_and_sixteen_seconds() {
    let file = fixture("ts-holder");
    let mut holder = ts("hold", &file, "60000");
    assert_eq!(holder.line(), "LOCKED");
    let lock = PathBuf::from(format!("{}.lock", file.display()));
    let before = fs::metadata(&lock).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let backend = FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap();
    assert!(
        backend
            .with_lock(Box::new(|_| Ok(Some("{\"stolen\":true}".to_owned()))))
            .is_err()
    );
    std::thread::sleep(Duration::from_secs(13));
    assert!(
        fs::metadata(&lock).unwrap().modified().unwrap() > before,
        "healthy TS heartbeat must renew mtime"
    );
    assert!(backend.with_lock(Box::new(|_| Ok(None))).is_err());
    fs::write(format!("{}.release", file.display()), "").unwrap();
    assert_eq!(holder.line(), "RELEASED");
    holder.finish();
    backend
        .with_lock(Box::new(|_| Ok(Some("{\"after\":true}".to_owned()))))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&fs::read_to_string(file).unwrap()).unwrap(),
        json!({"after":true})
    );
}
#[test]
fn rust_holder_child() {
    let Ok(path) = std::env::var("AUTH_PHASE1_HOLDER") else {
        return;
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let release = PathBuf::from(format!("{path}.release"));
            FileAuthStorageBackend::new(&JsString::from(&path))
                .unwrap()
                .with_lock_async(Box::new(move |_| {
                    Box::pin(async move {
                        println!("LOCKED");
                        let deadline = std::time::Instant::now() + Duration::from_secs(60);
                        while !release.exists() && std::time::Instant::now() < deadline {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                        Ok(None)
                    })
                }))
                .await
                .unwrap();
        });
    println!("RELEASED");
}
#[test]
fn healthy_rust_lock_excludes_ts_sync_at_three_and_sixteen_seconds() {
    let file = fixture("rust-holder");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "rust_holder_child", "--nocapture"])
        .env("AUTH_PHASE1_HOLDER", &file);
    let mut holder = Peer::spawn(command);
    while holder.line() != "LOCKED" {}
    let lock = PathBuf::from(format!("{}.lock", file.display()));
    let before = fs::metadata(&lock).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let mut contender = ts("try-sync", &file, "");
    assert_eq!(contender.line(), "ELOCKED");
    contender.finish();
    std::thread::sleep(Duration::from_secs(13));
    assert!(fs::metadata(&lock).unwrap().modified().unwrap() > before);
    let mut contender = ts("try-sync", &file, "");
    assert_eq!(contender.line(), "ELOCKED");
    contender.finish();
    fs::write(format!("{}.release", file.display()), "").unwrap();
    while holder.line() != "RELEASED" {}
    holder.finish();
    let mut contender = ts("try-sync", &file, "");
    assert_eq!(contender.line(), "ACQUIRED");
    contender.finish();
}
#[test]
fn owned_crashed_lock_recovers_only_after_thirty_seconds_stale() {
    let file = fixture("crash");
    let mut holder = ts("hold", &file, "60000");
    assert_eq!(holder.line(), "LOCKED");
    holder.kill();
    let lock = PathBuf::from(format!("{}.lock", file.display()));
    let stamp = SystemTime::now() - Duration::from_secs(31);
    fs::File::open(&lock)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    let backend = FileAuthStorageBackend::new(&JsString::from(file.to_str().unwrap())).unwrap();
    backend.with_lock(Box::new(|_| Ok(None))).unwrap();
}
