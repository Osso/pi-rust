use crate::shared_oracle as oracle;

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxTokenSize, RegisterFauxProviderOptions, create_faux_core, faux_assistant_message,
};
use pi_ai::request::{ProviderStreams, SimpleStreamOptions};
use pi_ai::types::Context;
use pi_ai::utils::estimate::estimate_context_tokens;
use pi_js_compat::js_json::{JsString, JsValue, from_value, parse_lossless, to_value};

const HARNESS_CONTEXT: &str = r#"{"systemPrompt":"You are a helpful assistant.","messages":[{"role":"user","content":[{"type":"text","text":"hello\ud800"}],"timestamp":0}],"tools":[{"name":"lookup","label":"lookup","description":"fixture","parameters":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}}]}"#;

#[tokio::test]
async fn agent_tool_metadata_matches_frozen_whole_object_consumers() {
    let mut oracle = oracle::TsOracle::spawn("tool-metadata-oracle.ts", "packages/ai/src");
    for (index, (label, execution_mode)) in [
        (Some(JsString::from("lookup")), None),
        (
            Some(JsString::from_utf16(vec![0xd800, 0xd83d, 0xde00])),
            Some("parallel"),
        ),
        (Some(JsString::from("lookup")), Some("sequential")),
        (None, None),
        (Some(JsString::from("lookup")), Some("parallel")),
    ]
    .into_iter()
    .enumerate()
    {
        let mut fixture = parse_lossless(HARNESS_CONTEXT).unwrap();
        let tool = fixture.get_mut("tools").unwrap().as_array_mut().unwrap()[0]
            .as_object_mut()
            .unwrap();
        match label {
            Some(label) => {
                tool.insert("label".into(), JsValue::String(label));
            }
            None => {
                tool.shift_remove("label");
            }
        }
        if let Some(mode) = execution_mode {
            tool.insert("executionMode".into(), JsValue::String(mode.into()));
        }
        if index == 4 {
            tool.insert(
                "description".into(),
                JsValue::String(JsString::from_utf16(vec![0xd800, 0x61, 0xdfff])),
            );
        }
        let expected = oracle.request(&pi_js_compat::js_json!({"context":fixture}));
        let context: Context = from_value(&fixture).unwrap();
        let core = create_faux_core(RegisterFauxProviderOptions {
            api: Some("test-api".into()),
            provider: Some("test-provider".into()),
            models: Some(vec![]),
            token_size: Some(FauxTokenSize {
                min: Some(1.0),
                max: Some(1.0),
            }),
            ..Default::default()
        });
        core.set_responses(vec![
            faux_assistant_message(
                "reply",
                FauxAssistantMessageOptions {
                    timestamp: Some(0.0),
                    ..Default::default()
                },
            )
            .into(),
        ]);
        let stream = core
            .stream_simple(
                core.get_model(Some("")).unwrap(),
                context.clone(),
                SimpleStreamOptions::default(),
            )
            .expect("acquisition");
        let result = stream.result().await.unwrap();
        if index == 0 {
            assert_eq!(expected["input"].as_f64(), Some(53.0), "shared harness fixture");
        }
        assert_eq!(
            result.usage.input,
            expected["input"].as_f64().unwrap(),
            "case {index}: harness tool metadata reaches Faux token counting"
        );
        assert_eq!(
            serde_json::to_string(&context.tools).unwrap(),
            expected["toolWire"].as_string().unwrap().to_utf8().unwrap(),
            "case {index}: exact frozen enumerable key order"
        );
        assert_eq!(
            to_value(&estimate_context_tokens(&context)).unwrap(),
            expected["estimate"],
            "case {index}: utils/estimate whole-tool counting"
        );
        let mapped =
            pi_ai::api::openai_responses_shared::convert_responses_tools(context.tools.as_ref().unwrap(), None);
        assert!(mapped[0].get("label").is_none());
        assert!(mapped[0].get("executionMode").is_none());
    }
}
