#[path = "differential_rpc_jsonl/mod.rs"]
mod rpc_jsonl_oracle;

#[test]
fn rpc_jsonl_matches_the_pinned_typescript_source() {
    rpc_jsonl_oracle::run();
}
