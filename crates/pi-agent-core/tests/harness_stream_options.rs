//! Behavioral port of the harness orchestration value types in `packages/agent/src/harness/types.ts`.

use indexmap::IndexMap;

use pi_agent_core::harness::types::{AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, OptionalPatch};
use pi_ai::types::{CacheRetention, Transport};
use serde_json::json;

#[test]
fn stream_options_keep_absent_and_explicit_values_distinct() {
    let options = AgentHarnessStreamOptions::default();
    assert_eq!(options.transport, None);
    assert_eq!(options.timeout_ms, None);
    assert_eq!(options.websocket_connect_timeout_ms, None);
    assert_eq!(options.max_retries, None);
    assert_eq!(options.max_retry_delay_ms, None);
    assert!(options.headers.is_none());
    assert!(options.metadata.is_none());
    assert_eq!(options.cache_retention, None);
    assert!(options.on_retry.is_none());

    let options = AgentHarnessStreamOptions {
        transport: Some(Transport::WebsocketCached),
        timeout_ms: Some(30_000.0),
        websocket_connect_timeout_ms: Some(5_000.0),
        max_retries: Some(2.0),
        max_retry_delay_ms: Some(1_000.0),
        headers: Some(
            [
                ("zeta".to_owned(), "1".to_owned()),
                ("alpha".to_owned(), "2".to_owned()),
            ]
            .into_iter()
            .collect(),
        ),
        metadata: Some(json!({"trace": "abc"}).as_object().unwrap().clone()),
        cache_retention: Some(CacheRetention::Long),
        on_retry: None,
    };
    assert_eq!(options.transport, Some(Transport::WebsocketCached));
    assert_eq!(options.timeout_ms, Some(30_000.0));
    assert_eq!(options.max_retries, Some(2.0));
    assert_eq!(options.headers.as_ref().unwrap()["zeta"], "1");
    let header_keys: Vec<&str> = options.headers.as_ref().unwrap().keys().map(String::as_str).collect();
    assert_eq!(header_keys, vec!["zeta", "alpha"]);
    assert_eq!(options.metadata.as_ref().unwrap()["trace"], "abc");
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
    headers.insert("keep".to_owned(), Some("value".to_owned()));
    headers.insert("delete".to_owned(), None);
    let mut metadata = IndexMap::new();
    metadata.insert("keep".to_owned(), Some(json!(1)));
    metadata.insert("delete".to_owned(), None);
    let set = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        metadata: OptionalPatch::Set(metadata.into_iter().collect()),
        timeout_ms: Some(100.0),
        ..Default::default()
    };
    let OptionalPatch::Set(headers) = &set.headers else {
        panic!("headers must be set");
    };
    assert_eq!(headers["delete"], None);
    assert_eq!(set.timeout_ms, Some(100.0));
}

#[test]
fn patch_headers_use_an_ordered_map() {
    let mut headers = IndexMap::new();
    headers.insert("b".to_owned(), None);
    headers.insert("a".to_owned(), Some("1".to_owned()));
    let patch = AgentHarnessStreamOptionsPatch {
        headers: OptionalPatch::Set(headers.clone()),
        ..Default::default()
    };
    assert_eq!(patch.headers, OptionalPatch::Set(headers));
}
