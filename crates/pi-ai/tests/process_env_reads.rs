//! `process.env` reads observe runtime overlay mutations like Node.

use pi_ai::env_api_keys::get_env_api_key;
use pi_js_compat::process_env;

#[test]
fn provider_env_keys_observe_process_env_overlay() {
    process_env::set("ZAI_API_KEY", "overlay-key");
    let found = get_env_api_key("zai", None);
    process_env::remove("ZAI_API_KEY");

    assert_eq!(
        found.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("overlay-key"))
    );
}
