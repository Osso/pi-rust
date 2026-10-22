//! Process-boundary harness for the real resident startup helper; no production-only test hooks.
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = if args.first().map(String::as_str) == Some("ensure") {
        let settings = pi_coding_agent::core::host_relay_client::RelaySettings {
            peer: "fake-peer".into(),
            remote_command: None,
        };
        pi_coding_agent::core::host_relay_resident::ensure_relay_running(&args[1].as_str().into(), Some(&settings))
            .await
            .map(|()| 0)
            .map_err(JsString::from)
    } else {
        pi_coding_agent::main(&args, Default::default()).await
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{}", encode_node_utf8(&error));
            std::process::exit(1);
        }
    }
}
