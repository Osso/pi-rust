#[path = "differential_config_value_references/mod.rs"]
mod config_value_reference_oracle;
#[path = "support/config_value_status_probe.rs"]
mod config_value_status_probe;

#[test]
fn config_value_references_match_the_pinned_typescript_source() {
    config_value_reference_oracle::run();
}

#[test]
#[ignore]
fn config_status_process_probe_child() {
    config_value_status_probe::run_child_probe();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
