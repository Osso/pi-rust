//! Differential test for `packages/ai/src/compat.ts` against the pinned module.

use std::sync::{Arc, Mutex};

use pi_ai::compat::{
    ApiProvider, complete, get_api_provider, get_api_providers, register_api_provider, register_faux_provider,
    reset_api_providers, stream, unregister_api_providers,
};
use pi_ai::providers::faux::{FauxAssistantMessageOptions, faux_assistant_message};
use pi_ai::request::{ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Message, Modality, Model,
    ModelCost, ModelCostRates, ProviderEnv, StopReason, TextContent, ThinkingLevel, UserContent, UserMessage, UserRole,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};
use serde_json::{Value, json};

#[path = "support/oracle.rs"]
mod oracle;

const SOURCE_MODULE: &str = "packages/ai/src/compat.ts";

static REGISTRY_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    REGISTRY_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("tokio runtime")
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("hi there".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        })],
        tools: None,
    }
}

fn model(api: &str, provider: &str, id: &str) -> Model {
    Model {
        properties: Default::default(),
        id: id.to_owned(),
        name: "Test Model".into(),
        api: api.to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.test/v1".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 128_000.0,
        auto_compaction_threshold: None,
        max_tokens: 4096.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn ok_message(model: &Model) -> AssistantMessage {
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, 0.0);
    message.content = vec![AssistantContent::Text(TextContent::new("ok"))];
    message.stop_reason = StopReason::Stop;
    message
}

fn fake_stream(model: &Model) -> AssistantMessageEventStream {
    let message = ok_message(model);
    let (sender, stream) = create_assistant_message_event_stream();
    sender.push(AssistantMessageEvent::Start {
        partial: message.clone(),
    });
    sender.push(AssistantMessageEvent::Done {
        reason: DoneReason::Stop,
        message,
    });
    sender.end(None);
    stream
}

type Seen = Arc<Mutex<Vec<(Option<pi_js_compat::js_json::JsString>, Option<ThinkingLevel>)>>>;

fn fake_provider(api: &str, seen: Seen) -> ApiProvider {
    let stream_seen = Arc::clone(&seen);
    let simple_seen = Arc::clone(&seen);
    ApiProvider::new(
        api,
        move |model, _context, options| {
            stream_seen
                .lock()
                .unwrap()
                .push((options.request.api_key.clone(), None));
            Ok(fake_stream(&model))
        },
        move |model, _context, options| {
            simple_seen
                .lock()
                .unwrap()
                .push((options.stream.request.api_key.clone(), options.reasoning));
            Ok(fake_stream(&model))
        },
    )
}

#[test]
fn registry_operations_match_the_pinned_typescript_module() {
    let _guard = serial();
    let mut oracle = oracle::TsOracle::spawn("compat-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id": 1, "op": "registry"}));
    assert_eq!(expected["ok"], true, "{expected}");

    reset_api_providers();
    let initial = api_ids();
    register_api_provider(
        fake_provider("openai-responses", Seen::default()),
        Some("source-a".to_owned()),
    );
    register_api_provider(
        fake_provider("openai-completions", Seen::default()),
        Some("source-b".to_owned()),
    );
    let after_register = api_ids();
    unregister_api_providers("source-a");
    let after_unregister = api_ids();
    reset_api_providers();
    let after_reset = api_ids();

    let actual = json!({
        "initial": initial,
        "afterRegister": after_register,
        "afterUnregister": after_unregister,
        "afterReset": after_reset,
    });
    assert_eq!(expected["output"], actual);
}

fn api_ids() -> Vec<String> {
    get_api_providers()
        .iter()
        .map(|provider| provider.api().to_owned())
        .collect()
}

#[test]
fn dispatch_injects_env_keys_like_the_pinned_typescript_module() {
    let _guard = serial();
    let cases = json!([
        {"name": "explicit", "api": "openai-responses", "provider": "custom-openai", "modelId": "test-model", "options": {"apiKey": "request-key"}},
        {"name": "env", "api": "openai-responses", "provider": "openai", "modelId": "gpt-5", "options": {"env": {"OPENAI_API_KEY": "env-key"}}},
        {"name": "explicit-wins", "api": "openai-responses", "provider": "openai", "modelId": "gpt-5", "options": {"apiKey": "request-key", "env": {"OPENAI_API_KEY": "env-key"}}},
        {"name": "blank", "api": "openai-responses", "provider": "openai", "modelId": "gpt-5", "options": {"apiKey": "   ", "env": {"OPENAI_API_KEY": "env-key"}}},
        {"name": "no-key", "api": "openai-responses", "provider": "custom-openai", "modelId": "test-model", "options": {}},
        {"name": "ambient", "api": "bedrock-converse-stream", "provider": "amazon-bedrock", "modelId": "custom", "options": {"env": {"AWS_PROFILE": "local-fixture"}}},
        {"name": "simple-ambient", "api": "bedrock-converse-stream", "provider": "amazon-bedrock", "modelId": "custom", "simple": true, "options": {"env": {"AWS_PROFILE": "local-fixture"}}},
        {"name": "simple-env", "api": "openai-responses", "provider": "ant-ling", "modelId": "test-model", "simple": true, "options": {"reasoning": "high", "env": {"ANT_LING_API_KEY": "env-key"}}},
        {"name": "simple-explicit", "api": "openai-responses", "provider": "ant-ling", "modelId": "test-model", "simple": true, "options": {"apiKey": "request-key", "reasoning": "low"}}
    ]);
    let mut oracle = oracle::TsOracle::spawn("compat-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id": 1, "op": "dispatch", "cases": cases}));
    assert_eq!(expected["ok"], true, "{expected}");

    let mut results = Vec::new();
    for case in cases.as_array().expect("cases") {
        let name = case["name"].as_str().expect("name");
        let api = case["api"].as_str().expect("api");
        let provider = case["provider"].as_str().expect("provider");
        let model_id = case["modelId"].as_str().expect("modelId");
        let simple = case["simple"].as_bool().unwrap_or(false);
        let options = &case["options"];

        reset_api_providers();
        let seen: Seen = Seen::default();
        register_api_provider(fake_provider(api, Arc::clone(&seen)), None);
        let request_model = model(api, provider, model_id);
        let api_key = options["apiKey"].as_str().map(str::to_owned);
        let env = options["env"].as_object().map(|env| {
            env.iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        pi_js_compat::js_json::JsString::from(value.as_str().expect("env value")),
                    )
                })
                .collect::<ProviderEnv>()
        });
        let reasoning = options["reasoning"].as_str().map(|level| match level {
            "minimal" => ThinkingLevel::Minimal,
            "low" => ThinkingLevel::Low,
            "medium" => ThinkingLevel::Medium,
            "high" => ThinkingLevel::High,
            "xhigh" => ThinkingLevel::Xhigh,
            "max" => ThinkingLevel::Max,
            "ultra" => ThinkingLevel::Ultra,
            other => panic!("unknown thinking level {other}"),
        });
        let request = ProviderRequestOptions {
            api_key: api_key.map(pi_js_compat::js_json::JsString::from),
            env,
            ..ProviderRequestOptions::default()
        };
        let message = if simple {
            runtime()
                .block_on(pi_ai::compat::complete_simple(
                    request_model,
                    context(),
                    SimpleStreamOptions {
                        stream: StreamOptions {
                            request,
                            ..StreamOptions::default()
                        },
                        reasoning,
                        ..SimpleStreamOptions::default()
                    },
                ))
                .expect("completeSimple")
        } else {
            runtime()
                .block_on(complete(
                    request_model,
                    context(),
                    StreamOptions {
                        request,
                        ..StreamOptions::default()
                    },
                ))
                .expect("complete")
        };
        let seen: Vec<Value> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|(api_key, reasoning)| {
                json!({
                    "apiKey": api_key,
                    "reasoning": reasoning.map(|level| serde_json::to_value(level).expect("thinking level")),
                })
            })
            .collect();
        results.push(json!({
            "name": name,
            "seen": seen,
            "content": message.content,
            "stopReason": serde_json::to_value(message.stop_reason).expect("stop reason"),
            "errorMessage": message.error_message,
        }));
    }
    reset_api_providers();
    assert_eq!(expected["output"]["results"], Value::Array(results));
}

#[test]
fn mismatch_and_missing_provider_errors_match_the_pinned_typescript_module() {
    let _guard = serial();
    let mut oracle = oracle::TsOracle::spawn("compat-oracle.ts", SOURCE_MODULE);

    let expected_mismatch = oracle.request(&json!({"id": 1, "op": "mismatch"}));
    reset_api_providers();
    register_api_provider(fake_provider("openai-responses", Seen::default()), None);
    let provider = get_api_provider("openai-responses").expect("registered");
    let mismatch = ProviderStreams::stream(
        &*provider,
        model("other-api", "custom", "m"),
        context(),
        StreamOptions::default(),
    );
    let error = mismatch.err().expect("mismatch rejects before a stream exists");
    assert_eq!(
        expected_mismatch["output"]["error"],
        json!({"name":error.name,"message":error.message})
    );

    let expected_missing = oracle.request(&json!({"id": 2, "op": "missing"}));
    reset_api_providers();
    let missing = stream(model("missing-api", "custom", "m"), context(), StreamOptions::default());
    let error = match missing {
        Ok(_) => panic!("missing provider must throw before returning a stream"),
        Err(error) => error,
    };
    assert_eq!(expected_missing["output"]["error"]["message"], json!(error.message));
    reset_api_providers();
}

#[test]
fn faux_registration_through_compat_matches_the_pinned_typescript_module() {
    let _guard = serial();
    let mut oracle = oracle::TsOracle::spawn("compat-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id": 1, "op": "faux"}));
    assert_eq!(expected["ok"], true, "{expected}");

    reset_api_providers();
    let registration = register_faux_provider(pi_ai::providers::faux::RegisterFauxProviderOptions {
        token_size: Some(pi_ai::providers::faux::FauxTokenSize {
            min: Some(3.0),
            max: Some(5.0),
        }),
        ..Default::default()
    });
    registration.set_responses(vec![
        faux_assistant_message("hello world", FauxAssistantMessageOptions::default()).into(),
    ]);
    let target = registration.get_model(None).expect("default faux model");
    let message = runtime()
        .block_on(complete(target, context(), StreamOptions::default()))
        .expect("complete");
    let output = json!({
        "apiPrefix": registration.api().starts_with("faux:"),
        "content": message.content,
        "inputPositive": message.usage.input > 0.0,
        "outputPositive": message.usage.output > 0.0,
        "totalMatches": message.usage.total_tokens == message.usage.input + message.usage.output,
        "callCount": registration.state().lock().unwrap().call_count,
        "pending": registration.get_pending_response_count(),
    });
    registration.unregister();
    let after_unregister = api_ids();
    reset_api_providers();

    assert_eq!(expected["output"]["output"], output);
    assert_eq!(
        expected["output"]["afterUnregister"],
        Value::Array(after_unregister.into_iter().map(Value::String).collect())
    );
}

mod nss {
    use super::oracle;
    use pi_ai::request::{SimpleStreamOptions, StreamOptions};
    use pi_ai::types::Context;
    use serde_json::{Value, json};
    use std::process::Command;

    #[test]
    fn compat_stream_acquisition_throws_vertex_homedir_error_synchronously() {
        let directory = std::env::temp_dir().join(format!("compat-nss-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let library = directory.join("passwd-failure.so");
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
            "compat-nss-oracle.mts",
            "packages/ai/src/compat.ts",
            &[("LD_PRELOAD", library.as_os_str().to_owned())],
        );
        let mut model = pi_ai::compat::get_model("google-vertex", "gemini-2.5-pro").unwrap();
        model.id = "nss-custom-model".into();
        for operation in ["stream", "streamSimple"] {
            let expected: Value = source.request(&json!({"operation":operation,"model":model}));
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "nss::compat_nss_child", "--nocapture"])
                .env_remove("HOME")
                .env_remove("GOOGLE_APPLICATION_CREDENTIALS")
                .env_remove("GOOGLE_CLOUD_API_KEY")
                .env("LD_PRELOAD", &library)
                .env("PI_TEST_FAIL_PASSWD", "1")
                .env("PI_TEST_COMPAT_OPERATION", operation)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            let actual: Value =
                serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
            assert_eq!(actual, expected, "{operation}");
            assert_eq!(actual["threw"], true);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "isolated HOME/passwd child"]
    fn compat_nss_child() {
        let mut model = pi_ai::compat::get_model("google-vertex", "gemini-2.5-pro").unwrap();
        model.id = "nss-custom-model".into();
        let operation = std::env::var("PI_TEST_COMPAT_OPERATION").unwrap();
        let context = Context {
            messages: vec![],
            system_prompt: None,
            tools: None,
        };
        let result = match operation.as_str() {
            "stream" => pi_ai::compat::stream(model, context, StreamOptions::default()),
            "streamSimple" => pi_ai::compat::stream_simple(model, context, SimpleStreamOptions::default()),
            _ => panic!("unexpected operation {operation}"),
        };
        let value = match result {
            Ok(_) => json!({"threw":false}),
            Err(error) => {
                let native = match error.thrown_value() {
                    pi_js_compat::abort::AbortReason::Error(native)
                    | pi_js_compat::abort::AbortReason::DomException(native) => native,
                    _ => panic!("NSS fixture must throw a native Error"),
                };
                json!({"threw":true,"message":native.message,"error":native.string_value()})
            }
        };
        println!("RESULT:{value}");
    }
}
