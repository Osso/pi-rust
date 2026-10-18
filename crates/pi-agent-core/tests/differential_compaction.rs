#[path = "differential_compaction/mod.rs"]
mod compaction_oracle;

#[tokio::test(flavor = "multi_thread")]
async fn compaction_matches_the_pinned_typescript_source() {
    compaction_oracle::run().await;
}
