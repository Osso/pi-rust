use super::*;
use pi_ai::request::SimpleStreamOptions;

#[tokio::test]
async fn nonobject_tool_arguments_fail_explicitly_without_losing_scratch() {
    for text in ["null", "[]", "42", "true", "\"scalar\""] {
        let input = json!({"events":[
            {"type":"content_block_start","index":7,"content_block":{"type":"tool_use","id":"c","name":"Read","input":{}}},
            {"type":"content_block_delta","index":7,"delta":{"type":"input_json_delta","partial_json":text}},
            {"type":"content_block_stop","index":7},
            {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{}}
        ]});
        let provider = provider(&input, Arc::new(Mutex::new(vec![]))).await;
        let mut stream = provider.stream_simple(
            provider.get_models()[0].clone(),
            serde_json::from_value(json!({"messages":[]})).unwrap(),
            SimpleStreamOptions::default(),
        );
        let mut events = vec![];
        while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
            .await
            .expect("nonobject input must terminate, not strand the stream after a producer panic")
        {
            events.push(serde_json::to_value(event.unwrap()).unwrap());
        }
        assert_eq!(
            events.iter().map(|e| e["type"].as_str().unwrap()).collect::<Vec<_>>(),
            ["start", "toolcall_start", "error"],
            "{text}: {events:?}"
        );
        let error = &events[2]["error"];
        assert_eq!(error["errorMessage"], "Tool arguments must be a JSON object");
        assert_eq!(error["content"][0]["arguments"], json!({}));
        assert_eq!(error["content"][0]["partialJson"], text);
        assert!(error["content"][0].get("index").is_none());
        assert_eq!(serde_json::to_value(stream.result().await.unwrap()).unwrap(), *error);
    }
}

#[tokio::test]
async fn literal_lifecycle_keeps_unclosed_scratch_and_first_duplicate_index() {
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    for failure in [false, true] {
        let mut input = json!({"events":[
            {"type":"content_block_start","index":3,"content_block":{"type":"text","text":"ignored"}},
            {"type":"content_block_start","index":3,"content_block":{"type":"text","text":"ignored"}},
            {"type":"content_block_delta","index":3,"delta":{"type":"text_delta","text":"first"}},
            {"type":"content_block_stop","index":3},
            {"type":"content_block_delta","index":3,"delta":{"type":"text_delta","text":"second"}},
            {"type":"content_block_start","index":8,"content_block":{"type":"tool_use","id":"c","name":"Read","input":{"ignored":true}}},
            {"type":"content_block_delta","index":8,"delta":{"type":"input_json_delta","partial_json":"{\"a\":1}"}},
            {"type":"content_block_delta","index":8,"delta":{"type":"input_json_delta","partial_json":"broken"}},
            {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{}}
        ]});
        if failure {
            input["streamError"] = json!("wire failure");
        }
        let expected = oracle.request(&input);
        let provider = provider(&input, Arc::new(Mutex::new(vec![]))).await;
        let mut stream = provider.stream_simple(
            provider.get_models()[0].clone(),
            serde_json::from_value(json!({"messages":[]})).unwrap(),
            SimpleStreamOptions::default(),
        );
        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(serde_json::to_value(event.unwrap()).unwrap());
        }
        assert_eq!(events, expected["events"].as_array().unwrap().clone());
        let last = events.last().unwrap();
        let message = last.get("message").or_else(|| last.get("error")).unwrap();
        assert_eq!(message["content"][0]["text"], "first");
        assert_eq!(message["content"][1]["text"], "second");
        assert_eq!(message["content"][2]["arguments"], json!({"a":1}));
        assert_eq!(message["content"][2]["partialJson"], "{\"a\":1}broken");
        assert_eq!(message["content"][2].get("index").is_some(), !failure);
    }
}
