use crate::shared_oracle as oracle;
use pi_js_compat::{
    abort::{AbortController, AbortReason},
    js_json::{JsString, JsValue},
};
use pi_tui::autocomplete::{CombinedAutocompleteProvider, SuggestionsOptions};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const FD: &str = r#"#!/usr/bin/python3
import os, pathlib, sys, time
root = pathlib.Path(__file__).parent
if os.fork() == 0:
    (root / 'held.pid').write_text(str(os.getpid()))
    deadline = time.monotonic() + 10
    while not (root / 'release').exists() and time.monotonic() < deadline:
        time.sleep(0.002)
    (root / 'released').write_text('closed')
    os._exit(0)
(root / 'parent.pid').write_text(str(os.getpid()))
sys.stdout.write('file.txt\n')
sys.stdout.flush()
os._exit(0)
"#;
struct Fixture {
    directory: tempfile::TempDir,
    fd: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let fd = directory.path().join("fd");
        std::fs::write(&fd, FD).unwrap();
        std::fs::set_permissions(&fd, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(directory.path().join("file.txt"), "fixture").unwrap();
        Self { directory, fd }
    }
    fn base(&self) -> &Path {
        self.directory.path()
    }
    fn release(&self) {
        std::fs::write(self.base().join("release"), "release").unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::write(self.base().join("release"), "release");
        if self.base().join("held.pid").exists() {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !self.base().join("released").exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
async fn native(fixture: &Fixture, abort: bool, custom_reason: bool) -> Value {
    let controller = AbortController::new();
    let signal = controller.signal();
    let provider = CombinedAutocompleteProvider::new(
        vec![],
        fixture.base().to_str().unwrap(),
        Some(fixture.fd.to_str().unwrap().into()),
    );
    let lines = vec![JsString::from("@file")];
    let pending = provider.get_suggestions(
        &lines,
        0,
        5,
        SuggestionsOptions {
            signal: Some(&signal),
            force: false,
        },
    );
    tokio::pin!(pending);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // Poll completion before checking the exit marker: child.wait has advanced
        // to stdout's EOF wait when the parent has been reaped and the pipe is held.
        tokio::select! { biased;
            result=&mut pending=>panic!("fd completed before pipe release: {result:?}"),
            _=tokio::time::sleep(Duration::from_millis(5))=>{}
        }
        let parent = std::fs::read_to_string(fixture.base().join("parent.pid")).ok();
        if let Some(pid) = parent
            && fixture.base().join("held.pid").exists()
            && !Path::new(&format!("/proc/{}/stat", pid.trim())).exists()
        {
            break;
        }
        assert!(Instant::now() < deadline, "fd parent was not reaped before abort");
    }
    let held = !fixture.base().join("released").exists();
    if abort {
        controller.abort(custom_reason.then_some(AbortReason::Value(JsValue::Bool(false))));
    }
    fixture.release();
    let result = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("fd pipe did not close");
    let result = result
        .map(|result| {
            json!([
                result
                    .items
                    .iter()
                    .map(|item| json!([item.value, item.label, item.description]))
                    .collect::<Vec<_>>(),
                result.prefix
            ])
        })
        .unwrap_or(Value::Null);
    json!({"parentGone":true,"pipeHeld":held,"aborted":signal.aborted(),"customReason":signal.reason().is_some_and(|reason|matches!(reason,AbortReason::Value(JsValue::Bool(false)))),"result":result})
}
#[tokio::test(flavor = "current_thread")]
async fn abort_after_parent_exit_but_before_stdout_close_matches_frozen() {
    let mut oracle = oracle::TsOracle::spawn("autocomplete-close-abort-oracle.mts", "packages/tui/src");
    for (abort, custom_reason) in [(false, false), (true, false), (true, true)] {
        let source = Fixture::new();
        let expected = oracle
            .request(&json!({"base":source.base().to_str().unwrap(),"fd":source.fd.to_str().unwrap(),"abort":abort,"customReason":custom_reason}));
        assert_eq!(expected["ok"], true, "source {expected}");
        let fixture = Fixture::new();
        let actual = native(&fixture, abort, custom_reason).await;
        assert_eq!(actual, expected["output"], "abort={abort}");
        if abort {
            assert!(actual["result"].is_null());
        } else {
            assert_eq!(actual["result"][0].as_array().unwrap().len(), 1);
        }
    }
}
