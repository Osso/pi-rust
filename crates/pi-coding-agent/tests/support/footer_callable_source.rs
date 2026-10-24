#[path = "mode_harness.rs"]
mod mode_harness;

use std::path::Path;
use std::process::{Command, Output};

use pi_js_compat::js_json::{JsValue, stringify_lossless};

pub fn run(operation: &str, directory: &Path, model: JsValue) -> Output {
    let lock = mode_harness::read_source_lock();
    mode_harness::verify_source_head(&lock);
    let request = pi_js_compat::js_json!({"operation":operation,"cwd":directory.to_str().unwrap(),"model":model});
    let request_path = directory.join("footer-request.json");
    std::fs::write(&request_path, stringify_lossless(&request)).unwrap();
    Command::new("/usr/bin/node")
        .arg(lock.worktree.join("node_modules/tsx/dist/cli.mjs"))
        .arg("--tsconfig")
        .arg(lock.worktree.join("tsconfig.json"))
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/footer-callable-status-oracle.mts"))
        .arg(lock.worktree.join("packages/coding-agent/src"))
        .arg(request_path)
        .output()
        .unwrap()
}
