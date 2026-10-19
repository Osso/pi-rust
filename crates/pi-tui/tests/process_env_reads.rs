//! Terminal environment detection observes runtime `process.env` overlay mutations.

use pi_js_compat::process_env;
use pi_tui::tui_alt_screen::AltScreenEnvironment;

#[test]
fn alt_screen_detection_observes_process_env_overlay() {
    process_env::set("TMUX", "/tmp/tmux-1000/default,123,0");
    process_env::set("TERM", "screen-256color");
    let environment = AltScreenEnvironment::detect();
    process_env::remove("TMUX");
    process_env::remove("TERM");

    assert!(environment.tmux);
    assert_eq!(environment.term.as_deref(), Some("screen-256color"));
}
