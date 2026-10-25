use crate::shared_harness_support as support;

#[path = "differential_harness_core/loaders.rs"]
mod loaders;
#[path = "differential_harness_core/messages.rs"]
mod messages;

#[tokio::test]
async fn harness_messages_and_loaders_match_the_pinned_typescript_source() {
    let mut oracle = support::spawn_oracle();
    messages::compare_all_fixtures(&mut oracle);
    loaders::compare_all_fixtures(&mut oracle).await;
}
