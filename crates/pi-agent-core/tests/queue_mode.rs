use pi_agent_core::harness::QueueMode as RootQueueMode;
use pi_agent_core::harness::types::QueueMode;

#[test]
fn variants_return_exact_source_literals() {
    assert_eq!(QueueMode::All.as_str(), "all");
    assert_eq!(QueueMode::OneAtATime.as_str(), "one-at-a-time");
}

#[test]
fn enum_values_support_equality_copy_and_debug() {
    let mut mode = QueueMode::All;
    let copied = mode;

    assert_eq!(mode, copied);
    assert_eq!(format!("{mode:?}"), "All");

    mode = QueueMode::OneAtATime;
    assert_eq!(mode, QueueMode::OneAtATime);
    assert_eq!(format!("{mode:?}"), "OneAtATime");
}

#[test]
fn harness_root_reexports_the_same_enum() {
    assert_eq!(RootQueueMode::All, QueueMode::All);
    assert_eq!(RootQueueMode::OneAtATime, QueueMode::OneAtATime);
    assert_eq!(RootQueueMode::All.as_str(), "all");
    assert_eq!(RootQueueMode::OneAtATime.as_str(), "one-at-a-time");
}
