//! Behavioral port of the harness orchestration value types in `packages/agent-core/src/harness/types.ts`.

use indexmap::IndexMap;

use pi_agent_core::harness::types::{AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, OptionalPatch};
use pi_ai::types::{CacheRetention, Transport};
use pi_js_compat::{js_json as json, js_json::JsString};

#[test]
fn stream_options_keep_absent_and_explicit_values_distinct() {
    let options = AgentHarnessStreamOptions::default();
    assert_eq!(options.transport, None);
    assert_eq!(options.timeout_ms, None);
    assert_eq!(options.max_retries, None);
    assert_eq!(options.max_retry_delay_ms, None);
    assert!(options.headers.is_none());
    assert!(options.metadata.is_none());
    assert_eq!(options.cache_retention, None);

    let options = AgentHarnessStreamOptions {
        transport: Some(Transport::WebsocketCached),
        timeout_ms: Some(30_000.0),
        max_retries: Some(2.0),
        max_retry_delay_ms: Some(1_000.0),
        headers: Some(
            [
                (JsString::from("zeta"), JsString::from("1")),
                (JsString::from("alpha"), JsString::from("2")),
            ]
            .into_iter()
            .collect(),
        ),
        metadata: Some(json!({"trace": "abc"}).as_object().unwrap().clone()),
        cache_retention: Some(CacheRetention::Long),
    };
    assert_eq!(options.transport, Some(Transport::WebsocketCached));
    assert_eq!(options.timeout_ms, Some(30_000.0));
    assert_eq!(options.max_retries, Some(2.0));
    assert_eq!(options.headers.as_ref().unwrap()[&JsString::from("zeta")], "1");
    let header_keys: Vec<String> = options
        .headers
        .as_ref()
        .unwrap()
        .keys()
        .map(|key| key.to_utf8().unwrap())
        .collect();
    assert_eq!(header_keys, vec!["zeta", "alpha"]);
    assert_eq!(
        options.metadata.as_ref().unwrap()[&JsString::from("trace")],
        json!("abc")
    );
    assert_eq!(options.cache_retention, Some(CacheRetention::Long));
}

#[test]
fn stream_option_patch_distinguishes_unchanged_clear_and_set() {
    let patch = AgentHarnessStreamOptionsPatch::default();
    assert_eq!(patch.headers, OptionalPatch::Unchanged);
    assert_eq!(patch.metadata, OptionalPatch::Unchanged);

    let clear = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Clear,
        metadata: OptionalPatch::Clear,
        ..Default::default()
    };
    assert_eq!(clear.headers, OptionalPatch::Clear);
    assert_eq!(clear.metadata, OptionalPatch::Clear);

    let mut headers = IndexMap::new();
    headers.insert(JsString::from("keep"), Some(JsString::from("value")));
    headers.insert(JsString::from("delete"), None);
    let mut metadata = IndexMap::new();
    metadata.insert(JsString::from("keep"), Some(json!(1)));
    metadata.insert(JsString::from("delete"), None);
    let set = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        metadata: OptionalPatch::Set(metadata.into_iter().collect()),
        timeout_ms: OptionalPatch::Set(100.0),
        ..Default::default()
    };
    let OptionalPatch::Set(headers) = &set.headers else {
        panic!("headers must be set");
    };
    assert_eq!(headers[&JsString::from("delete")], None);
    assert_eq!(set.timeout_ms, OptionalPatch::Set(100.0));
}

#[test]
fn patch_headers_use_an_ordered_map() {
    let mut headers = IndexMap::new();
    headers.insert(JsString::from("b"), None);
    headers.insert(JsString::from("a"), Some(JsString::from("1")));
    let patch = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        ..Default::default()
    };
    assert_eq!(patch.headers, OptionalPatch::Set(headers));
}
