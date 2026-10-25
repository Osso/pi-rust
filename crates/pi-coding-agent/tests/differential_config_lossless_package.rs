#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::config::{self, ConfigError};
use pi_js_compat::{js_json as json, js_json::JsValue};
use std::{ffi::OsString, process::Command};

#[test]
fn package_metadata_and_derived_outputs_keep_frozen_utf16() {
    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        ("high-and-low", br#"{"name":"pkg\ud800","version":"1.0\udc00","piConfig":{"name":"app\ud800title","configDir":".cfg\udc00"}}"#.to_vec()),
        ("case-mapping", br#"{"name":"package","version":"2.0","piConfig":{"name":"\u00dfi\udc00","configDir":".home\ud800"}}"#.to_vec()),
        ("node-path-join", "{\"name\":\"package\",\"version\":\"3.0\",\"piConfig\":{\"name\":\"/a//../β\",\"configDir\":\"/outer/../.cfg\"}}".as_bytes().to_vec()),
        ("nul-name", br#"{"name":"package","version":"4.0","piConfig":{"name":"PI_PACKAGE_ENV\u0000name","configDir":".cfg"}}"#.to_vec()),
        ("defaults", b"{}".to_vec()),
        ("empty-fields", br#"{"name":"","version":"","piConfig":{"name":"","configDir":""}}"#.to_vec()),
        ("invalid-utf8", [b"{\"piConfig\":{\"name\":\"a".as_slice(), &[0xff], b"b\"}}"].concat()),
        ("syntax-utf16-window", "aaaaaaaaa😀bbbbbbbbbbbb".as_bytes().to_vec()),
    ];
    let mut mismatches = Vec::new();
    for (name, bytes) in fixtures {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("package.json"), bytes).unwrap();
        let mut oracle = oracle::TsOracle::spawn_with_environment(
            "config-lossless-package-oracle.mjs",
            "packages/coding-agent/src/config.ts",
            &[
                ("PI_PACKAGE_DIR", directory.path().as_os_str().to_owned()),
                ("HOME", OsString::from("/fixture-home")),
                ("NO_COLOR", OsString::from("1")),
            ],
        );
        let mut expected: JsValue = oracle.request(&json!({"cacheHome":"/cache/../normalized-cache"}));
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "package_snapshot_child", "--nocapture"])
            .env("PI_PACKAGE_DIR", directory.path())
            .env("HOME", "/fixture-home")
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let mut actual =
            json::parse_lossless(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        if name == "nul-name" {
            // Approved runtime gap: Node truncates environment names at NUL.
            assert_eq!(
                actual.as_object_mut().unwrap().remove("override"),
                Some(json!("/environment-agent"))
            );
            let frozen_default = expected["roots"]["getAgentDir"].clone();
            assert_eq!(
                expected.as_object_mut().unwrap().remove("override"),
                Some(frozen_default)
            );
        }
        if actual != expected {
            let different: Vec<_> = expected
                .as_object()
                .unwrap()
                .iter()
                .filter_map(|(key, value)| (actual.as_object().unwrap().get(key) != Some(value)).then_some(key.clone()))
                .collect();
            mismatches.push(format!("{name}: differing fields {different:?}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

fn run_package_probe() -> Result<JsValue, ConfigError> {
    let metadata = config::try_package_config().map_err(Clone::clone)?;
    let install = config::InstallContext::for_process()?;
    let layouts = json!({
        "compiled":install.is_bun_binary, "bunRuntime":install.is_bun_runtime,
        "method":config::detect_install_method(&install).as_str(),
        "themes":config::get_themes_dir()?, "exportTemplate":config::get_export_template_dir()?,
        "assets":config::get_interactive_assets_dir()?, "bundled":config::get_bundled_interactive_asset_path("logo.txt")?,
    });
    for name in [
        config::env_agent_dir(),
        config::env_session_dir(),
        config::env_state_dir(),
    ] {
        pi_js_compat::process_env::remove(&name);
    }
    for name in ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"] {
        pi_js_compat::process_env::remove(name);
    }
    pi_js_compat::process_env::set("XDG_CACHE_HOME", "/cache/../normalized-cache");
    let roots = json!({
        "getUserConfigRoot":config::get_user_config_root()?, "getUserStateRoot":config::get_user_state_root()?,
        "getUserCacheRoot":config::get_user_cache_root()?, "getLegacyAgentDir":config::get_legacy_agent_dir()?,
        "getAgentDir":config::get_agent_dir()?, "getAuthPath":config::get_auth_path()?, "getDebugLogPath":config::get_debug_log_path()?,
    });
    pi_js_compat::process_env::set(&config::env_agent_dir(), "/environment-agent");
    let overridden = config::get_agent_dir()?;
    pi_js_compat::process_env::remove(&config::env_agent_dir());
    Ok(json!({
        "packageName":metadata.package_name.clone(), "version":metadata.version.clone(), "appName":metadata.app_name.clone(),
        "appTitle":metadata.app_title.clone(), "configDirName":metadata.config_dir_name.clone(),
        "envNames":[config::env_agent_dir(), config::env_session_dir(), config::env_state_dir()],
        "layouts":layouts, "roots":roots, "override":overridden, "helpLabels":pi_coding_agent::cli::args::print_help(&[]).lines().filter(|line| line.contains("- Config directory (default:") || line.contains("- Session storage directory (overridden")).map(str::to_owned).collect::<Vec<_>>(),
    }))
}

#[test]
#[ignore = "isolated package constants child"]
fn package_snapshot_child() {
    let value = match run_package_probe() {
        Ok(value) => value,
        Err(error) => {
            let native = error.into_native_error();
            json!({"error":native.value(), "constructor":native.constructor_class().name(), "text":native.string_value()})
        }
    };
    println!("RESULT:{}", json::stringify_lossless(&value));
}
