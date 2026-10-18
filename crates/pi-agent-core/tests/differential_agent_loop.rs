#[path = "differential_agent_loop/mod.rs"]
mod agent_oracle;

#[tokio::test]
async fn agent_loop_scenarios_match_the_pinned_typescript_source() {
    agent_oracle::run_differential().await;
}
