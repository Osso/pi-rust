#[path = "../examples/extensions/notify.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use pi_coding_agent::core::extensions::ExtensionFactory;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::{io::Write, sync::Arc};

struct Output {
    trace: harness::Trace,
    fail: bool,
    pending: Vec<u8>,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.fail {
            return Err(std::io::Error::other("stdout closed"));
        }
        self.pending.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            self.trace
                .lock()
                .unwrap()
                .push(json!(["stdout", std::str::from_utf8(&self.pending).unwrap()]));
            self.pending.clear();
        }
        Ok(())
    }
}
#[tokio::test]
async fn protocols_precedence_external_fixture_and_output_errors_match_ts() {
    let temp = tempfile::tempdir().unwrap();
    let exe = temp.path().join("powershell.exe");
    std::fs::write(&exe, "#!/usr/bin/python3\nimport os,sys,json\np=os.environ['NOTIFY_RECEIPT']\nwith open(p+'.tmp','w') as f: json.dump(sys.argv[1:],f)\nos.rename(p+'.tmp',p)\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let receipt = temp.path().join("receipt.json");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        pi_js_compat::process_env::get("PATH").unwrap()
    );
    for (wt, kitty, fail) in [("", "", false), ("", "1", false), ("1", "1", false), ("", "", true)] {
        pi_js_compat::process_env::set("WT_SESSION", wt);
        pi_js_compat::process_env::set("KITTY_WINDOW_ID", kitty);
        pi_js_compat::process_env::set("PATH", &path);
        pi_js_compat::process_env::set("NOTIFY_RECEIPT", receipt.to_str().unwrap());
        let factory = move |trace: harness::Trace| {
            if !wt.is_empty() {
                return harness::factory(example::notify_extension)(trace);
            }
            let factory: ExtensionFactory = Arc::new(move |api| {
                let result = example::notify_with_output(
                    api,
                    Output {
                        trace: trace.clone(),
                        fail,
                        pending: Vec::new(),
                    },
                );
                Box::pin(async move { result })
            });
            factory
        };
        let mut steps = vec![json!({"op":"event","event":{"type":"agent_end","messages":[]}})];
        if !wt.is_empty() {
            steps.push(json!({"op":"receipt"}));
        }
        let actual = harness::compare("notify",factory,json!({"fileLoader":true,"notifyEnv":{"WT_SESSION":wt,"KITTY_WINDOW_ID":kitty,"PATH":path,"NOTIFY_RECEIPT":receipt},"execReceipt":receipt,"writeError":if fail {Some("stdout closed")} else {None},"steps":steps})).await;
        if fail {
            assert_eq!(actual["trace"], json!([["handlerError", "stdout closed"]]));
        } else if !wt.is_empty() {
            assert_eq!(actual["trace"][0][0], "exec");
            assert_eq!(actual["trace"][0][1][0], "-NoProfile");
        } else if !kitty.is_empty() {
            assert_eq!(actual["trace"].as_array().unwrap().len(), 2);
        } else {
            assert_eq!(
                actual["trace"],
                json!([["stdout", "\u{1b}]777;notify;Pi;Ready for input\u{7}"]])
            );
        }
    }
    // Node execFile without a callback silently ignores missing executables.
    // Exercise that boundary with a private empty PATH, never host commands.
    let empty = temp.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    pi_js_compat::process_env::set("WT_SESSION", "1");
    pi_js_compat::process_env::set("PATH", empty.to_str().unwrap());
    let actual = harness::compare(
        "notify",
        |trace: harness::Trace| {
            let factory: ExtensionFactory = Arc::new(move |api| {
                let result = example::notify_with_output(
                    api,
                    Output {
                        trace: trace.clone(),
                        fail: false,
                        pending: Vec::new(),
                    },
                );
                Box::pin(async move { result })
            });
            factory
        },
        json!({"fileLoader":true,"notifyEnv":{"WT_SESSION":"1","PATH":empty},"steps":[
            {"op":"event","event":{"type":"agent_end","messages":[]}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"], json!([]));
    pi_js_compat::process_env::clear();
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
