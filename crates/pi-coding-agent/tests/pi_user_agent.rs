use pi_coding_agent::utils::pi_user_agent::{
    format_pi_user_agent, get_pi_user_agent, node_arch_name, node_platform_name,
};

#[test]
fn formats_the_user_agent_expected_by_pi_dev() {
    assert_eq!(
        format_pi_user_agent("1.2.3", "linux", "node/v20.11.0", "x64"),
        "pi/1.2.3 (linux; node/v20.11.0; x64)"
    );
    assert_eq!(
        format_pi_user_agent("0.84.1", "darwin", "bun/1.1.30", "arm64"),
        "pi/0.84.1 (darwin; bun/1.1.30; arm64)"
    );
}

#[test]
fn maps_rust_platform_names_to_node_names() {
    assert_eq!(node_platform_name("linux"), "linux");
    assert_eq!(node_platform_name("macos"), "darwin");
    assert_eq!(node_platform_name("windows"), "win32");
    assert_eq!(node_platform_name("freebsd"), "freebsd");
    assert_eq!(node_platform_name("openbsd"), "openbsd");
}

#[test]
fn maps_rust_arch_names_to_node_names() {
    assert_eq!(node_arch_name("x86_64"), "x64");
    assert_eq!(node_arch_name("aarch64"), "arm64");
    assert_eq!(node_arch_name("x86"), "ia32");
    assert_eq!(node_arch_name("arm"), "arm");
    assert_eq!(node_arch_name("powerpc64"), "ppc64");
    assert_eq!(node_arch_name("s390x"), "s390x");
    assert_eq!(node_arch_name("riscv64"), "riscv64");
}

#[test]
fn runtime_user_agent_has_the_three_segment_shape() {
    let user_agent = get_pi_user_agent("1.2.3");
    assert!(user_agent.starts_with("pi/1.2.3 ("), "{user_agent}");
    assert!(user_agent.ends_with(')'), "{user_agent}");
    let inner = &user_agent["pi/1.2.3 (".len()..user_agent.len() - 1];
    assert_eq!(inner.split(';').count(), 3, "{user_agent}");
    assert!(!inner.contains('('), "{user_agent}");
}
