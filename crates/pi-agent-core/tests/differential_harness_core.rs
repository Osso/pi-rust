#[path = "differential_harness_core/support.rs"]
mod support;

#[path = "differential_harness_core/events.rs"]
mod events;
#[path = "differential_harness_core/messages.rs"]
mod messages;

#[test]
fn harness_events_and_messages_match_the_pinned_typescript_source() {
    let mut oracle = events::spawn_oracle();
    events::compare_all_fixtures(&mut oracle);
    messages::compare_all_fixtures(&mut oracle);
}
