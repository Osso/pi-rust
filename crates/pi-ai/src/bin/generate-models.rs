//! Explicit maintenance entrypoint; never invoked during ordinary catalog loading.
use pi_ai::model_catalog_generator::{ConsoleLog, fetch_native, run_generator};
use pi_js_compat::js_json::encode_node_utf8;
use std::{path::Path, process::ExitCode};

fn emit(log: &ConsoleLog) {
    let message = encode_node_utf8(&log.render());
    if log.is_error {
        eprintln!("{message}");
    } else {
        println!("{message}");
    }
}
fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("pi-ai lives under crates/");
    let mut fetch = |url: &str| runtime.block_on(fetch_native(url));
    // The source ignores argv and attaches console.error to its top-level catch.
    if let Err(error) = run_generator(root, &mut fetch, &mut emit) {
        emit(&ConsoleLog {
            is_error: true,
            args: vec![error.thrown_value()],
        });
    }
    ExitCode::SUCCESS
}
