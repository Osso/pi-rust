#[path = "differential_telemetry/mod.rs"]
mod telemetry_oracle;

#[test]
fn telemetry_matches_the_pinned_typescript_source() {
    telemetry_oracle::run();
}
