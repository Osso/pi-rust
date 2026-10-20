//! Explicit maintenance entrypoint; never invoked during normal model loading.
use pi_ai::model_catalog_generator::{fetch_native, run_generator_cli};
use pi_js_compat::{
    date::{js_now_millis, to_iso_string},
    js_json::{JsString, JsValue, to_js_string},
};
use std::{path::Path, process::ExitCode};

fn emit(log: &JsValue) {
    let JsValue::Array(parts) = log else {
        unreachable!("generator emits console tuples");
    };
    let values = parts[1..]
        .iter()
        .map(|part| to_js_string(part).expect("console tuple strings").to_utf8_lossy())
        .collect::<Vec<_>>();
    if parts.first() == Some(&JsValue::String(JsString::from("error"))) {
        eprintln!("{}", values.join(" "));
    } else {
        println!("{}", values.join(" "));
    }
}
fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    // Invalid arguments are rejected before any transport setup or requests.
    if let Err(error) = pi_ai::model_catalog_generator::read_generator_options(&args) {
        eprintln!("{}", error.console_message());
        return ExitCode::FAILURE;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut fetch = |url: &str| runtime.block_on(fetch_native(url));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("pi-ai lives under crates/");
    let generated_at = JsString::from(to_iso_string(js_now_millis() as i64).as_str());
    match run_generator_cli(&args, root, &generated_at, &mut fetch, &mut emit) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.console_message());
            ExitCode::FAILURE
        }
    }
}
