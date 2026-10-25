use crate::shared_oracle as oracle;

use pi_ai::types::ProviderHeaders;
use pi_ai::utils::headers::{headers_to_record, provider_headers_to_record};
use serde_json::{Value, json};

fn header_requests() -> Vec<Value> {
    vec![
        json!({"id": 0, "operation": "headers", "pairs": [["X-Test", "one"], ["x-test", "two"], ["Accept", "text/plain"]]}),
        json!({"id": 1, "operation": "headers", "pairs": [["X-Empty", ""]]}),
        json!({"id": 2, "operation": "headers", "pairs": []}),
        json!({"id": 6, "operation": "headers", "pairs": [["X-Space", "  padded\t"], ["X-Space", " next "]]}),
    ]
}

fn provider_requests() -> Vec<Value> {
    vec![
        json!({"id": 3, "operation": "provider", "headers": {"x-keep": "value", "x-drop": null}}),
        json!({"id": 4, "operation": "provider", "headers": {"x-drop": null}}),
        json!({"id": 5, "operation": "provider", "headers": null}),
    ]
}

fn record_keys(value: &Value) -> Vec<String> {
    value["output"]["record"]
        .as_object()
        .expect("record object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn header_record_order_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("headers-oracle.ts", "packages/ai/src/utils/headers.ts");

    // `Headers` sorts names lexicographically, so the record follows sorted order.
    let headers_request = json!({
        "id": 10,
        "operation": "headers",
        "pairs": [["Zed", "1"], ["alpha", "2"], ["Middle", "3"]]
    });
    let pairs: Vec<(String, String)> = headers_request["pairs"]
        .as_array()
        .expect("pairs")
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().expect("name").to_owned(),
                pair[1].as_str().expect("value").to_owned(),
            )
        })
        .collect();
    let observed = json!({"id": 10, "ok": true, "output": {"record": headers_to_record(pairs)}});
    let expected = oracle.request(&headers_request);
    assert_eq!(observed, expected, "headers record values");
    assert_eq!(record_keys(&observed), record_keys(&expected), "headers record order");

    // `providerHeadersToRecord` iterates the source object, preserving insertion order.
    let provider_request = json!({
        "id": 11,
        "operation": "provider",
        "headers": {"zeta": "Z", "alpha": "A", "middle": null, "beta": "B"}
    });
    let headers: Option<ProviderHeaders> = provider_request["headers"].as_object().map(|object| {
        object
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
            .collect()
    });
    let record = provider_headers_to_record(headers.as_ref());
    let observed = json!({"id": 11, "ok": true, "output": {"record": record}});
    let expected = oracle.request(&provider_request);
    assert_eq!(observed, expected, "provider header values");
    assert_eq!(
        record_keys(&observed),
        vec!["zeta", "alpha", "beta"],
        "provider header order"
    );
    assert_eq!(
        record_keys(&observed),
        record_keys(&expected),
        "provider header oracle order"
    );
}

#[test]
fn header_records_match_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("headers-oracle.ts", "packages/ai/src/utils/headers.ts");
    for request in header_requests() {
        let pairs: Vec<(String, String)> = request["pairs"]
            .as_array()
            .expect("pairs")
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().expect("name").to_owned(),
                    pair[1].as_str().expect("value").to_owned(),
                )
            })
            .collect();
        let observed = json!({"id": request["id"], "ok": true, "output": {"record": headers_to_record(pairs)}});
        assert_eq!(observed, oracle.request(&request), "request {request}");
    }

    for request in provider_requests() {
        let headers: Option<ProviderHeaders> = request["headers"].as_object().map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
                .collect()
        });
        let record = provider_headers_to_record(headers.as_ref());
        let observed = json!({"id": request["id"], "ok": true, "output": {"record": record}});
        assert_eq!(observed, oracle.request(&request), "request {request}");
    }
}
