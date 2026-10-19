//! Config reads observe runtime `process.env` overlay mutations like Node.

use pi_coding_agent::config::get_share_viewer_url;
use pi_js_compat::process_env;

#[test]
fn share_viewer_url_observes_process_env_overlay() {
    process_env::set("PI_SHARE_VIEWER_URL", "https://example.test/view/");
    let url = get_share_viewer_url("abc");
    process_env::remove("PI_SHARE_VIEWER_URL");

    assert_eq!(url, "https://example.test/view/#abc");
}
