use super::*;
use pi_ai::request::{SimpleStreamOptions, StreamOptions};

fn options(input: &Value, signal: CancellationToken) -> SimpleStreamOptions {
    let mut options = SimpleStreamOptions::default();
    options.stream.request.signal = Some(signal);
    options.stream.request.api_key = input["options"]["apiKey"].as_str().map(str::to_owned);
    options.stream.max_tokens = input["options"]["maxTokens"].as_f64();
    options.reasoning = input["options"]
        .get("reasoning")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .unwrap();
    options.thinking_budgets = input["options"]
        .get("thinkingBudgets")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .unwrap();
    if let Some(number) = input["numericBudget"].as_str() {
        options.thinking_budgets = Some(pi_ai::types::ThinkingBudgets {
            high: Some(number.parse().unwrap()),
            ..Default::default()
        });
    }
    options
}
fn terminal(reason: &str) -> Value {
    json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"input_tokens":11,"output_tokens":7,"cache_read_input_tokens":5,"cache_creation_input_tokens":3}})
}
async fn compare(input: Value, oracle: &mut oracle::TsOracle) -> Vec<Value> {
    let expected = oracle.request(&input);
    let calls = Arc::new(Mutex::new(vec![]));
    let provider = provider(&input, calls.clone()).await;
    let mut model = provider.get_models()[input["modelIndex"].as_u64().unwrap_or(0) as usize].clone();
    if input["model"]["reasoning"] == false {
        model.reasoning = false;
    }
    let context = serde_json::from_value(input.get("context").cloned().unwrap_or(json!({"messages":[]}))).unwrap();
    let signal = CancellationToken::new();
    let options = options(&input, signal);
    let mut stream = if input["raw"] == true {
        provider.stream(
            model,
            context,
            StreamOptions {
                request: options.stream.request,
                max_tokens: options.stream.max_tokens,
                ..Default::default()
            },
        )
    } else {
        provider.stream_simple(model, context, options)
    };
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(serde_json::to_value(event.unwrap()).unwrap());
    }
    assert_eq!(
        events,
        expected["events"].as_array().unwrap().clone(),
        "event sequence fixture {input}"
    );
    assert_eq!(
        serde_json::to_value(stream.result().await.unwrap()).unwrap(),
        *events
            .last()
            .unwrap()
            .get("message")
            .or_else(|| events.last().unwrap().get("error"))
            .unwrap()
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{input}");
    assert_eq!(
        calls[0]["body"], expected["calls"][1]["params"],
        "request fixture {input}"
    );
    let headers = calls[0]["headers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect::<serde_json::Map<String, Value>>();
    let client = &expected["calls"][0]["options"];
    for (key, value) in client["defaultHeaders"].as_object().unwrap() {
        assert_eq!(headers[key], *value, "{input}");
    }
    if let Some(key) = client["authToken"].as_str() {
        assert_eq!(headers["authorization"], format!("Bearer {key}"));
        assert!(!headers.contains_key("x-api-key"));
    } else {
        assert_eq!(headers["x-api-key"], client["apiKey"]);
        assert!(!headers.contains_key("authorization"));
    }
    assert_eq!(headers["x-stainless-helper-method"], "stream");
    events
}
fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    )
}
fn assistant(content: Value) -> Value {
    json!({"role":"assistant","content":content,"api":"other","provider":"other","model":"old","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":0})
}
fn result(id: &str, content: Value, error: bool) -> Value {
    json!({"role":"toolResult","toolCallId":id,"toolName":"read","content":content,"isError":error,"timestamp":0})
}

struct Env(Value);
impl AuthContext for Env {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        assert_eq!(name, "CUSTOM_ANTHROPIC_API_KEY");
        let value = self.0[name].as_str().map(str::to_owned);
        Box::pin(async move { value })
    }
    fn file_exists(&self, _: &str) -> BoxFuture<'_, bool> {
        panic!("env auth must not read files")
    }
}
#[tokio::test]
async fn native_registered_api_key_auth_matches_pinned_composed_registration() {
    let mut oracle = oracle();
    for mut input in [
        json!({"env":{"CUSTOM_ANTHROPIC_API_KEY":"FAKE-from-env"}}),
        json!({"env":{}}),
        json!({"env":{"CUSTOM_ANTHROPIC_API_KEY":""}}),
        json!({"env":{"CUSTOM_ANTHROPIC_API_KEY":"FAKE-env"},"credential":{"type":"api_key","key":"FAKE-stored","env":{"EXTRA":"FAKE-extra"}}}),
        json!({"credential":{"type":"api_key","key":""}}),
    ] {
        input["kind"] = json!("providerAuth");
        for check in [false, true] {
            input["check"] = json!(check);
            let expected = oracle.request(&input);
            let provider = provider(&input, Arc::new(Mutex::new(vec![]))).await;
            let ctx: Arc<dyn AuthContext> = Arc::new(Env(input["env"].clone()));
            let credential = input
                .get("credential")
                .cloned()
                .map(serde_json::from_value::<Credential>)
                .transpose()
                .unwrap()
                .and_then(|c| if let Credential::ApiKey(c) = c { Some(c) } else { None });
            let auth = provider.auth().api_key.as_ref().unwrap();
            let result = if check {
                match auth.check(ApiKeyCheckInput {
                    ctx,
                    credential,
                    signal: CancellationToken::new(),
                }) {
                    Some(future) => future.await.map(|r| serde_json::to_value(r).unwrap()),
                    None => Ok(Value::Null),
                }
            } else {
                auth.resolve(ApiKeyResolveInput {
                    ctx,
                    credential,
                    signal: CancellationToken::new(),
                })
                .await
                .map(|r| serde_json::to_value(r).unwrap())
            };
            let actual = match result {
                Ok(result) => json!({"name":provider.name(),"result":result}),
                Err(e) => json!({"name":provider.name(),"error":e.to_string()}),
            };
            assert_eq!(actual, expected, "{input}");
        }
    }
}

#[tokio::test]
async fn pinned_native_numeric_budgets_preserve_nonfinite_null_serialization() {
    let mut oracle = oracle();
    for number in ["NaN", "Infinity", "-Infinity"] {
        compare(json!({"events":[terminal("end_turn")],"options":{"apiKey":"FAKE-key","reasoning":"high"},"numericBudget":number}),&mut oracle).await;
    }
}

#[tokio::test]
async fn pinned_request_conversion_matrix() {
    let mut oracle = oracle();
    let image = json!({"type":"image","data":"RkFLRQ==","mimeType":"image/png"});
    let context = json!({"systemPrompt":"sys😀","tools":[{"name":"rEaD","description":"read tool","parameters":{"type":"object","properties":{"file":{"type":"string"}},"required":["file"],"additionalProperties":false}},{"name":"EnterPlanMode","description":"unchanged","parameters":{}}],"messages":[
        {"role":"user","content":" \t\u{FEFF}","timestamp":0}, {"role":"user","content":[],"timestamp":0},
        {"role":"user","content":[image.clone(),{"type":"text","text":"hi😀"}],"timestamp":0},
        assistant(json!([{"type":"text","text":" "},{"type":"thinking","thinking":" "}])),
        assistant(json!([{"type":"text","text":"hello😀"},{"type":"thinking","thinking":"unsigned😀"},{"type":"thinking","thinking":"signed😀","thinkingSignature":"sig😀"},{"type":"toolCall","id":"c1","name":"rEaD","arguments":{"file":"a"}},{"type":"toolCall","id":"c2","name":"EnterPlanMode","arguments":{}}])),
        result("c1",json!([{"type":"text","text":"one😀"},{"type":"text","text":"two"}]),false),result("c2",json!([image.clone()]),true),result("c3",json!([{"type":"text","text":"caption"},image.clone()]),false),
        {"role":"user","content":"later","timestamp":0}, result("c4",json!([]),false)
    ]});
    for key in ["FAKE-api", "prefix-sk-ant-oat-FAKE-token"] {
        compare(json!({"events":[terminal("end_turn")],"context":context,"options":{"apiKey":key,"maxTokens":0,"reasoning":"minimal"}}),&mut oracle).await;
    }
    for level in ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        for budgets in [Value::Null, json!({"minimal":0,"low":17,"medium":22,"high":-3})] {
            let mut input = json!({"events":[terminal("max_tokens")],"options":{"apiKey":"FAKE-key","reasoning":level,"maxTokens":42},"modelIndex":1});
            if !budgets.is_null() {
                input["options"]["thinkingBudgets"] = budgets;
            }
            compare(input, &mut oracle).await;
        }
    }
    for context in [
        json!({"messages":[],"tools":[],"systemPrompt":""}),
        json!({"messages":[{"role":"user","content":[image],"timestamp":0}]}),
    ] {
        compare(json!({"events":[terminal("stop_sequence")],"context":context,"model":{"reasoning":false},"options":{"reasoning":"high","apiKey":"sk-ant-oat-FAKE"}}),&mut oracle).await;
    }
    compare(json!({"events":[terminal("pause_turn")],"raw":true}), &mut oracle).await;
}

#[tokio::test]
async fn pinned_stream_content_usage_and_error_matrix() {
    let mut oracle = oracle();
    let events = json!([
        {"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":2,"cache_read_input_tokens":30,"cache_creation_input_tokens":40}}},
        {"type":"content_block_start","index":4,"content_block":{"type":"text","text":"ignored"}},
        {"type":"content_block_delta","index":99,"delta":{"type":"text_delta","text":"ignored"}},
        {"type":"content_block_delta","index":4,"delta":{"type":"thinking_delta","thinking":"ignored"}},
        {"type":"content_block_delta","index":4,"delta":{"type":"text_delta","text":"hello 😀"}},
        {"type":"content_block_start","index":1,"content_block":{"type":"thinking","thinking":"ignored","signature":"ignored"}},
        {"type":"content_block_delta","index":1,"delta":{"type":"thinking_delta","thinking":"reason"}},
        {"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"sig"}},
        {"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"nature"}},
        {"type":"content_block_start","index":8,"content_block":{"type":"tool_use","id":"c1","name":"Read","input":{"ignored":true}}},
        {"type":"content_block_delta","index":8,"delta":{"type":"input_json_delta","partial_json":"{\"file\":"}},
        {"type":"content_block_delta","index":8,"delta":{"type":"input_json_delta","partial_json":"\"a\"}"}},
        {"type":"content_block_delta","index":8,"delta":{"type":"input_json_delta","partial_json":"garbage"}},
        {"type":"content_block_start","index":9,"content_block":{"type":"redacted_thinking","data":"ignored"}},
        {"type":"content_block_stop","index":9}, {"type":"content_block_stop","index":8},
        {"type":"content_block_stop","index":1}, {"type":"content_block_stop","index":4},
        {"type":"content_block_delta","index":4,"delta":{"type":"text_delta","text":"ignored after stop"}},
        terminal("tool_use"), {"type":"message_stop"}
    ]);
    compare(json!({"events":events,"options":{"apiKey":"sk-ant-oat-FAKE"},"context":{"messages":[],"tools":[{"name":"rEaD","description":"d","parameters":{}}]}}),&mut oracle).await;
}

#[tokio::test]
async fn pinned_incomplete_tool_failure_preserves_partial_json() {
    compare(json!({"events":[{"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"c2","name":"Write","input":{}}},{"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{"}}],"streamError":"incomplete tool"}),&mut oracle()).await;
}

#[tokio::test]
async fn pinned_terminal_usage_abort_and_error_matrix() {
    let mut oracle = oracle();
    for reason in [
        "end_turn",
        "pause_turn",
        "stop_sequence",
        "max_tokens",
        "tool_use",
        "refusal",
        "other",
    ] {
        compare(json!({"events":[terminal(reason)]}), &mut oracle).await;
    }
    for input in [
        json!({"events":[]}),
        json!({"events":[{"type":"message_delta","delta":{},"usage":{"output_tokens":4}}]}),
        json!({"events":[terminal("end_turn")],"abort":true}),
        json!({"events":[],"streamError":"wire failed"}),
        json!({"events":[],"streamError":{"why":"failure"},"thrownValue":true}),
    ] {
        compare(input, &mut oracle).await;
    }
    compare(json!({"events":[{"type":"message_start","message":{"usage":{"input_tokens":10,"output_tokens":2}}},{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}]}),&mut oracle).await;
}
