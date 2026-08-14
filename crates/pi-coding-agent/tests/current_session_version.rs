use pi_coding_agent::CURRENT_SESSION_VERSION;
use pi_coding_agent::core::session_manager::CURRENT_SESSION_VERSION as MODULE_CURRENT_SESSION_VERSION;

#[test]
fn current_session_version_matches_at_module_and_crate_root() {
    let module_value: f64 = MODULE_CURRENT_SESSION_VERSION;
    let root_value: f64 = CURRENT_SESSION_VERSION;

    assert_eq!(module_value.to_bits(), 3.0_f64.to_bits());
    assert_eq!(root_value.to_bits(), 3.0_f64.to_bits());
    assert_eq!(module_value.to_bits(), root_value.to_bits());

    let copied_module_value = MODULE_CURRENT_SESSION_VERSION;
    let copied_root_value = CURRENT_SESSION_VERSION;
    assert_eq!(copied_module_value.to_bits(), module_value.to_bits());
    assert_eq!(copied_root_value.to_bits(), root_value.to_bits());
}
