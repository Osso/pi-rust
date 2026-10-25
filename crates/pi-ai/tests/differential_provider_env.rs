use crate::shared_oracle as oracle;

use std::collections::BTreeMap;
use std::ffi::OsString;

use pi_ai::types::ProviderEnv;
use pi_ai::utils::provider_env::resolve_provider_env_value;
use serde_json::{Value, json};

fn process_env() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("PI_ORACLE_TEST_KEY".to_owned(), "process".to_owned()),
        ("PI_ORACLE_EMPTY_KEY".to_owned(), "".to_owned()),
    ])
}

fn requests() -> Vec<Value> {
    vec![
        json!({"id": 0, "name": "PI_ORACLE_TEST_KEY", "env": {"PI_ORACLE_TEST_KEY": "scoped"}}),
        json!({"id": 1, "name": "PI_ORACLE_TEST_KEY", "env": {"PI_ORACLE_TEST_KEY": ""}}),
        json!({"id": 2, "name": "PI_ORACLE_TEST_KEY", "env": null}),
        json!({"id": 3, "name": "PI_ORACLE_EMPTY_KEY", "env": null}),
        json!({"id": 4, "name": "PI_ORACLE_ABSENT_KEY", "env": null}),
        json!({"id": 5, "name": "PI_ORACLE_ABSENT_KEY", "env": {"PI_ORACLE_ABSENT_KEY": "scoped"}}),
    ]
}

#[test]
fn provider_env_resolution_matches_the_pinned_typescript_module() {
    let mut inherited_oracle =
        oracle::TsOracle::spawn("provider-env-oracle.ts", "packages/ai/src/utils/provider-env.ts");
    let inherited_request = json!({"id": 0, "name": "PI_ORACLE_TEST_KEY", "env": {"PI_ORACLE_TEST_KEY": "scoped"}});
    assert_eq!(
        json!({"id": 0, "ok": true, "output": {"value": "scoped"}}),
        inherited_oracle.request(&inherited_request),
        "scoped overrides win with an inherited process environment"
    );

    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "provider-env-oracle.ts",
        "packages/ai/src/utils/provider-env.ts",
        &[("PI_ORACLE_TEST_KEY", OsString::from("process"))],
    );
    let process = process_env();
    for request in requests() {
        let scoped: Option<ProviderEnv> = request["env"].as_object().map(|object| {
            object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        pi_js_compat::js_json::JsString::from(value.as_str().expect("string")),
                    )
                })
                .collect()
        });
        let value = resolve_provider_env_value(request["name"].as_str().expect("name"), scoped.as_ref(), |name| {
            process.get(name).cloned()
        });
        let observed = json!({"id": request["id"], "ok": true, "output": {"value": value}});
        assert_eq!(observed, oracle.request(&request), "request {request}");
    }
}
