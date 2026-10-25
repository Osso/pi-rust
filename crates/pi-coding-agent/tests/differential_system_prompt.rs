#[path = "differential_system_prompt/mod.rs"]
mod system_prompt_oracle;

#[test]
fn system_prompt_matches_the_pinned_typescript_source() {
    system_prompt_oracle::run();
}

#[test]
#[ignore = "child probe entry point"]
fn system_prompt_probe_child() {
    system_prompt_oracle::run_probe();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
