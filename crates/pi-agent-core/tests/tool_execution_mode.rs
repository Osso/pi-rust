use pi_agent_core::harness::ToolExecutionMode;
use pi_agent_core::harness::types::ToolExecutionMode as ModuleToolExecutionMode;

fn clone_value<T: Clone>(value: &T) -> T {
    value.clone()
}

#[test]
fn variants_return_exact_source_literals() {
    assert_eq!(ModuleToolExecutionMode::Sequential.as_str(), "sequential");
    assert_eq!(ModuleToolExecutionMode::Parallel.as_str(), "parallel");
}

#[test]
fn enum_values_support_equality_copy_and_debug() {
    let sequential = ModuleToolExecutionMode::Sequential;
    let copied = sequential;
    let cloned = clone_value(&sequential);

    assert_eq!(sequential, copied);
    assert_eq!(sequential, cloned);
    assert_eq!(format!("{sequential:?}"), "Sequential");
    assert_eq!(format!("{:?}", ModuleToolExecutionMode::Parallel), "Parallel");
}

#[test]
fn harness_root_reexports_the_same_enum() {
    let sequential: ToolExecutionMode = ToolExecutionMode::Sequential;
    let parallel: ToolExecutionMode = ToolExecutionMode::Parallel;

    assert_eq!(sequential.as_str(), "sequential");
    assert_eq!(parallel.as_str(), "parallel");
}
