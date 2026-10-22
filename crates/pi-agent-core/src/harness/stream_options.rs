//! Frozen agent-harness.ts request-option copy and patch semantics.

#[cfg(test)]
mod tests {
    use super::apply_stream_options_patch;
    use crate::harness::types::{AgentHarnessStreamOptions, AgentHarnessStreamOptionsPatch, OptionalPatch};
    use indexmap::IndexMap;
    use pi_ai::types::{CacheRetention, Transport};
    use pi_js_compat::js_json::{JsString, JsValue};

    #[test]
    fn clears_scalar_options_only_when_the_patch_owns_the_field() {
        let base = AgentHarnessStreamOptions {
            transport: Some(Transport::Sse),
            timeout_ms: Some(100.5),
            max_retries: Some(3.0),
            max_retry_delay_ms: Some(44.5),
            cache_retention: Some(CacheRetention::Long),
            ..Default::default()
        };
        let patch = AgentHarnessStreamOptionsPatch {
            transport: OptionalPatch::Clear,
            timeout_ms: OptionalPatch::Set(-1.5),
            max_retry_delay_ms: OptionalPatch::Clear,
            cache_retention: OptionalPatch::Clear,
            ..Default::default()
        };
        let result = apply_stream_options_patch(&base, Some(&patch));
        assert_eq!(result.transport, None);
        assert_eq!(result.timeout_ms, Some(-1.5));
        assert_eq!(result.max_retries, Some(3.0));
        assert_eq!(result.max_retry_delay_ms, None);
        assert_eq!(result.cache_retention, None);
        assert_eq!(base.transport, Some(Transport::Sse));
    }

    #[test]
    fn merges_and_deletes_native_header_and_metadata_keys_without_projection() {
        let raw = JsString::from_utf16(vec![0xd800, 0x21, 0xdfff]);
        let base = AgentHarnessStreamOptions {
            headers: Some(IndexMap::from([
                ("drop".into(), "old".into()),
                (raw.clone(), raw.clone()),
            ])),
            metadata: Some(IndexMap::from([
                ("drop".into(), JsValue::Null),
                (raw.clone(), JsValue::String(raw.clone())),
            ])),
            ..Default::default()
        };
        let patch = AgentHarnessStreamOptionsPatch {
            headers: OptionalPatch::Set(IndexMap::from([
                ("drop".into(), None),
                ("new".into(), Some(raw.clone())),
            ])),
            metadata: OptionalPatch::Set(IndexMap::from([
                ("drop".into(), None),
                ("new".into(), Some(JsValue::Null)),
            ])),
            ..Default::default()
        };
        let result = apply_stream_options_patch(&base, Some(&patch));
        let headers = result.headers.unwrap();
        assert_eq!(
            headers.keys().cloned().collect::<Vec<_>>(),
            vec![raw.clone(), "new".into()]
        );
        assert_eq!(headers.get(&raw), Some(&raw));
        assert_eq!(headers.get(&JsString::from("new")), Some(&raw));
        let metadata = result.metadata.unwrap();
        assert_eq!(metadata.get(&raw), Some(&JsValue::String(raw)));
        assert_eq!(metadata.get(&JsString::from("new")), Some(&JsValue::Null));
        assert_eq!(metadata.get(&JsString::from("drop")), None);
        assert_eq!(base.headers.unwrap().len(), 2);
    }

    #[test]
    fn removes_empty_maps_after_a_patch_and_preserves_unpatched_empty_maps() {
        let base = AgentHarnessStreamOptions {
            headers: Some(IndexMap::new()),
            metadata: Some(IndexMap::new()),
            ..Default::default()
        };
        let cloned = apply_stream_options_patch(&base, None);
        assert_eq!(cloned.headers, Some(IndexMap::new()));
        assert_eq!(cloned.metadata, Some(IndexMap::new()));
        let patch = AgentHarnessStreamOptionsPatch {
            headers: OptionalPatch::Set(IndexMap::new()),
            metadata: OptionalPatch::Set(IndexMap::new()),
            ..Default::default()
        };
        let patched = apply_stream_options_patch(&base, Some(&patch));
        assert_eq!(patched.headers, None);
        assert_eq!(patched.metadata, None);
    }
}
