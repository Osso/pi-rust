use super::oracle;
use pi_tui::autocomplete::{CombinedAutocompleteProvider, SuggestionsOptions};
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn home_lookup_errors_are_caught_for_file_and_fuzzy_suggestions() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pi-coding-agent/tests/support/passwd_failure.c");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut source = oracle::TsOracle::spawn_with_environment(
        "autocomplete-home-oracle.mts",
        "packages/tui/src/autocomplete.ts",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    for query in ["~/", "@~/"] {
        let expected: Value = source.request(&json!({"base":directory.path(),"query":query}));
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "nss_home::autocomplete_home_child",
                "--nocapture",
            ])
            .env_remove("HOME")
            .env("LD_PRELOAD", &library)
            .env("PI_TEST_FAIL_PASSWD", "1")
            .env("PI_TEST_AUTOCOMPLETE_QUERY", query)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual: Value =
            serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
        assert_eq!(actual, expected, "{query}");
        assert_eq!(actual, Value::Null);
    }
}

#[tokio::test]
#[ignore = "isolated HOME/passwd child"]
async fn autocomplete_home_child() {
    let query = pi_js_compat::js_json::JsString::from(std::env::var("PI_TEST_AUTOCOMPLETE_QUERY").unwrap());
    let provider = CombinedAutocompleteProvider::new(vec![], "/repo", Some("/nss-never-exec".into()));
    let result = provider
        .get_suggestions(
            &[query.clone()],
            0,
            query.len(),
            SuggestionsOptions {
                force: true,
                signal: None,
            },
        )
        .await;
    assert!(result.is_none(), "{result:?}");
    println!("RESULT:null");
}
