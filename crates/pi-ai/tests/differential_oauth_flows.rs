#[path = "differential_oauth_flows/mod.rs"]
mod oauth_flows;

#[test]
fn oauth_flows_match_the_pinned_typescript_source() {
    oauth_flows::run();
}
