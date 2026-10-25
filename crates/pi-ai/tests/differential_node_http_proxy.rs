use crate::shared_oracle as oracle;

use std::collections::BTreeMap;
use std::ffi::OsString;

use pi_ai::types::ProviderEnv;
use pi_ai::utils::node_http_proxy::resolve_http_proxy_url_for_target_with;
use serde_json::{Value, json};

fn process_environment() -> BTreeMap<String, String> {
    std::env::vars().collect()
}

fn requests() -> Vec<Value> {
    let proxies = [
        ("HTTP_PROXY", "http://proxy.example:8080"),
        ("HTTPS_PROXY", "https://secure-proxy.example:8443"),
        ("ALL_PROXY", "http://all.example:3128"),
        ("NO_PROXY", "excluded.example, .internal.example, 127.0.0.1:8080, [::1]"),
    ];
    let targets = [
        "http://example.com/",
        "http://example.com:8080/path",
        "https://example.com/",
        "https://example.com:8443/path",
        "https://excluded.example/",
        "https://api.internal.example/",
        "https://127.0.0.1:8080/",
        "https://127.0.0.1:9090/",
        "https://[::1]/",
        "ws://example.com/socket",
        "wss://example.com/socket",
        "ftp://example.com/file",
        "gopher://example.com/",
        "custom://example.com/",
        "mailto:user@example.com",
        "not a url",
        "",
    ];
    let mut requests = Vec::new();
    let mut id = 0;
    for target in targets {
        requests.push(json!({"id": id, "target": target, "env": null}));
        id += 1;
        requests.push(json!({
            "id": id,
            "target": target,
            "env": {
                "HTTP_PROXY": proxies[0].1,
                "HTTPS_PROXY": proxies[1].1,
                "ALL_PROXY": proxies[2].1,
                "NO_PROXY": proxies[3].1,
            }
        }));
        id += 1;
    }

    let scoped_cases = [
        json!({"HTTP_PROXY": "proxy.example:8080"}),
        json!({"https_proxy": "lower.example:8080"}),
        json!({"HTTPS_PROXY": "upper.example:8080", "https_proxy": "lower.example:8080"}),
        json!({"HTTPS_PROXY": ""}),
        json!({"ALL_PROXY": "socks5://proxy.example:1080"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "*"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "*.example.com"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": ".example.com"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "example.com:443"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "example.com:8443"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "a.test,b.test c.test"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "a.test,,b.test"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080", "NO_PROXY": "ample.com"}),
        json!({"HTTPS_PROXY": "http://user:pass@proxy.example:8080"}),
        json!({"HTTPS_PROXY": "http://proxy.example:8080/some/path"}),
    ];
    for scoped in scoped_cases {
        requests.push(json!({"id": id, "target": "https://example.com/", "env": scoped}));
        id += 1;
        requests.push(json!({"id": id, "target": "http://example.com/", "env": scoped}));
        id += 1;
    }
    requests
}

#[test]
fn invalid_targets_match_typescript() {
    let mut ts = oracle::TsOracle::spawn("node-http-proxy-oracle.ts", "packages/ai/src/utils/node-http-proxy.ts");
    let request = json!({"id": 0, "target": "not a url", "env": null});
    assert_eq!(
        json!({"id": 0, "ok": true, "output": {"url": null}}),
        ts.request(&request)
    );
}

#[test]
fn proxy_selection_matches_the_pinned_typescript_module() {
    let overrides: Vec<(&str, OsString)> = vec![
        ("HTTP_PROXY", OsString::from("http://process-http.example:8080")),
        ("HTTPS_PROXY", OsString::from("http://process-https.example:8080")),
        ("NO_PROXY", OsString::from("process-excluded.example")),
    ];
    let mut environment = process_environment();
    for (name, value) in &overrides {
        environment.insert(
            (*name).to_owned(),
            value.to_str().expect("override is UTF-8").to_owned(),
        );
    }
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "node-http-proxy-oracle.ts",
        "packages/ai/src/utils/node-http-proxy.ts",
        &overrides,
    );

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
        let target = request["target"].as_str().expect("target");
        let observed = match resolve_http_proxy_url_for_target_with(target, scoped.as_ref(), |name| {
            environment.get(name).cloned()
        }) {
            Ok(url) => json!({
                "id": request["id"],
                "ok": true,
                "output": {"url": url.map(|url| url.to_string())}
            }),
            Err(error) => json!({
                "id": request["id"],
                "ok": false,
                "error": {"name": "Error", "message": error.to_string()}
            }),
        };
        assert_eq!(observed, ts.request(&request), "request {request}");
    }
}
