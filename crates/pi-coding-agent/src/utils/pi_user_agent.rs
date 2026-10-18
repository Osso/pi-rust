/// Format the Pi user agent with explicit runtime metadata.
pub fn format_pi_user_agent(version: &str, platform: &str, runtime: &str, arch: &str) -> String {
    format!("pi/{version} ({platform}; {runtime}; {arch})")
}

/// Format the Pi user agent for this process.
///
/// The source reports the JavaScript runtime (`node/...` or `bun/...`). The
/// Rust port has no JavaScript runtime, so it reports `rust` in that position.
pub fn get_pi_user_agent(version: &str) -> String {
    format_pi_user_agent(
        version,
        node_platform_name(std::env::consts::OS),
        "rust",
        node_arch_name(std::env::consts::ARCH),
    )
}

/// Map a Rust target OS name to the `process.platform` spelling.
pub fn node_platform_name(os: &str) -> &str {
    match os {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

/// Map a Rust target architecture name to the `process.arch` spelling.
pub fn node_arch_name(arch: &str) -> &str {
    match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "powerpc64" => "ppc64",
        "loongarch64" => "loong64",
        "mips64" => "mips64el",
        other => other,
    }
}
