use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_coding_agent::core::{
    package_manager::{DefaultPackageManager, SettingsManagerStore},
    settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions},
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::Arc;

#[tokio::test]
async fn npm_version_selection_matches_source() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().join("project");
    let agent = directory.path().join("agent");
    let installed = agent.join("npm/node_modules/probe");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&installed).unwrap();
    let binary = directory.path().join("fake-npm");
    let output = directory.path().join("response");
    executable_script::write_executable_script(
        &binary,
        "#!/usr/bin/python3\nimport os,sys\nos.write(1,open(sys.argv[1],'rb').read())\n",
    );
    let command = vec![
        JsString::from(binary.to_str().unwrap()),
        JsString::from(output.to_str().unwrap()),
    ];
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let cases = [
        (
            "capture-bom",
            "npm:probe",
            "1.0.0",
            "\u{feff}\"2.0.0\"\u{feff}".to_owned(),
        ),
        (
            "stable-build",
            "npm:probe",
            "1.0.0+first",
            r#"["1.0.0+first","1.0.0+second"]"#.to_owned(),
        ),
        (
            "stable-range-build",
            "npm:probe@^1.0.0",
            "1.0.0+first",
            r#"["1.0.0+first","1.0.0+second"]"#.to_owned(),
        ),
        (
            "stable-v-prefix",
            "npm:probe",
            "v1.0.0",
            r#"["v1.0.0","1.0.0"]"#.to_owned(),
        ),
        (
            "stable-large-prerelease",
            "npm:probe",
            "1.0.0-9007199254740992",
            r#"["1.0.0-9007199254740992","1.0.0-9007199254740993"]"#.to_owned(),
        ),
        (
            "stable-range-large-prerelease",
            "npm:probe@>=1.0.0-9007199254740993",
            "1.0.0-9007199254740992",
            r#"["1.0.0-9007199254740992","1.0.0-9007199254740993"]"#.to_owned(),
        ),
        (
            "invalid-singleton",
            "npm:probe",
            "1.0.0",
            r#"["not-a-version"]"#.to_owned(),
        ),
        (
            "invalid-pair",
            "npm:probe",
            "1.0.0",
            r#"["not-a-version","2.0.0"]"#.to_owned(),
        ),
        (
            "invalid-raw-singleton",
            "npm:probe",
            "1.0.0",
            r#"["\ud800"]"#.to_owned(),
        ),
        (
            "range-skip-invalid",
            "npm:probe@^1.0.0",
            "1.0.0",
            r#"["not-a-version","1.1.0"]"#.to_owned(),
        ),
    ];
    for (name, package, installed_version, response) in cases {
        std::fs::write(&output, response).unwrap();
        std::fs::write(
            installed.join("package.json"),
            serde_json::json!({"name":"probe","version":installed_version}).to_string(),
        )
        .unwrap();
        let global = json!({"packages":[package],"npmCommand":command});
        let request = json!({
            "id":name, "operation":"check_for_available_updates",
            "scenario":{"cwd":cwd,"agentDir":agent,"global":global,"trusted":false}
        });
        let expected = source.request(&request);
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let mut values = Settings::new();
        values.insert("packages".into(), json!([package]));
        values.insert("npmCommand".into(), json!(command));
        let settings = SettingsManager::in_memory(values, SettingsManagerCreateOptions::default());
        let manager = DefaultPackageManager::new(
            &JsString::from(cwd.to_str().unwrap()),
            &JsString::from(agent.to_str().unwrap()),
            Arc::new(SettingsManagerStore::new(settings)),
        )
        .unwrap();
        let updates = manager.check_for_available_updates().await.unwrap();
        assert_eq!(json!({"updates":updates}), expected["output"], "{name}");
    }
}
