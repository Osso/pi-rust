#![allow(dead_code)] // Each integration test compiles its own copy and uses a subset.

//! Faux-provider fixtures for the eval tests.

use std::sync::Arc;

use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, FauxResponseStep, FauxTokenSize,
    RegisterFauxProviderOptions, faux_assistant_message, faux_provider, faux_text, faux_tool_call,
};
use pi_ai::types::{ModelCostRates, StopReason};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use serde_json::{Map, Value, json};

pub struct FauxFixture {
    pub runtime: ModelRuntime,
    pub faux: FauxProviderHandle,
}

fn faux_model() -> FauxModelDefinition {
    FauxModelDefinition {
        id: "faux-1".to_owned(),
        name: Some("Faux Model".to_owned()),
        reasoning: Some(false),
        input: None,
        cost: Some(ModelCostRates {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        }),
        context_window: Some(100_000.0),
        max_tokens: Some(4096.0),
    }
}

pub async fn faux_runtime() -> FauxFixture {
    let faux = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux".to_owned()),
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..Default::default()
    });
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(Map::from_iter([(
            "faux".to_owned(),
            json!({"type": "api_key", "key": "faux-key"}),
        )])))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("model runtime must initialize");
    runtime.register_native_provider(Arc::clone(&faux.provider));
    FauxFixture { runtime, faux }
}

pub fn tool_arguments(value: Value) -> Map<String, Value> {
    value.as_object().expect("tool arguments must be an object").clone()
}

/// A terminal assistant turn: text plus the `end_turn` call that ends the loop.
pub fn terminal_response(text: &str) -> FauxResponseStep {
    faux_assistant_message(
        vec![
            faux_text(text),
            faux_tool_call(
                "end_turn",
                tool_arguments(json!({"reason": "done"})),
                Some(format!("end-turn-{text}")),
            ),
        ],
        FauxAssistantMessageOptions {
            stop_reason: Some(StopReason::Stop),
            ..Default::default()
        },
    )
    .into()
}
