//! Package directory whose `pi` sibling is the built binary, so transcript/storage workers can spawn.

pub fn configure_worker() {
    static PACKAGE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let package = PACKAGE.get_or_init(|| {
        let package = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_pi"), package.path().join("pi")).unwrap();
        std::fs::create_dir(package.path().join("theme")).unwrap();
        for name in ["dark.json", "light.json"] {
            std::fs::copy(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("theme")
                    .join(name),
                package.path().join("theme").join(name),
            )
            .unwrap();
        }
        package
    });
    pi_js_compat::process_env::set("PI_PACKAGE_DIR", package.path().to_str().unwrap());
}
