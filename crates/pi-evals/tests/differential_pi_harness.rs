//! Differential tests for `evals/src/pi-harness.ts` model selection.

#[path = "support/oracle.rs"]
mod support;

use pi_evals::pi_harness::{ModelSelection, PiModelEnvironment, resolve_model_selection};
use serde_json::{Value, json};

const SOURCE_MODULE: &str = "packages/evals/src/pi-harness.ts";

fn selection(provider: &str, id: &str) -> ModelSelection {
    ModelSelection {
        provider: provider.to_owned(),
        id: id.to_owned(),
    }
}

fn environment(provider: Option<&str>, model: Option<&str>) -> PiModelEnvironment {
    PiModelEnvironment {
        pi_provider: provider.map(str::to_owned),
        pi_model: model.map(str::to_owned),
    }
}

fn fixtures() -> Vec<(Option<ModelSelection>, PiModelEnvironment)> {
    vec![
        (
            Some(selection("anthropic", "claude-opus-4-6")),
            environment(Some("openai-codex"), Some("gpt-5.6-sol")),
        ),
        (
            Some(selection("  anthropic  ", " claude-opus-4-6 ")),
            environment(Some("openai-codex"), Some("gpt-5.6-sol")),
        ),
        (None, environment(Some(" openai-codex "), Some(" gpt-5.6-sol "))),
        (None, environment(None, None)),
        (None, environment(Some("openai-codex"), None)),
        (None, environment(None, Some("gpt-5.6-sol"))),
        (
            Some(selection("", "gpt-5.6-sol")),
            environment(Some("openai-codex"), Some("gpt-5.6-sol")),
        ),
        (
            Some(selection("   ", "gpt-5.6-sol")),
            environment(Some("openai-codex"), Some("gpt-5.6-sol")),
        ),
        (
            Some(selection("anthropic", "")),
            environment(Some("openai-codex"), Some("gpt-5.6-sol")),
        ),
        (None, environment(Some(""), Some(""))),
        (None, environment(Some("\u{00a0}"), Some("\u{00a0}"))),
        (None, environment(Some("provider"), Some("  "))),
        (
            Some(selection("provider", "model")),
            environment(Some("ignored"), Some("ignored")),
        ),
    ]
}

#[test]
fn resolve_model_selection_matches_the_typescript_module() {
    let mut oracle = support::TsOracle::spawn("evals-pi-harness-oracle.ts", SOURCE_MODULE);
    for (index, (explicit, environment)) in fixtures().into_iter().enumerate() {
        let request = json!({
            "id": index,
            "operation": "resolve_model_selection",
            "explicitModel": explicit.as_ref().map(|model| json!({
                "provider": model.provider,
                "id": model.id,
            })),
            "environment": {
                "PI_PROVIDER": environment.pi_provider,
                "PI_MODEL": environment.pi_model,
            },
        });
        let response = oracle.request(&request);
        let actual = resolve_model_selection(explicit.as_ref(), &environment);
        match actual {
            Ok(selection) => {
                assert_eq!(response["ok"], true, "fixture {index}: {response}");
                assert_eq!(
                    response["output"],
                    json!({"provider": selection.provider, "id": selection.id}),
                    "fixture {index}"
                );
            }
            Err(error) => {
                assert_eq!(response["ok"], false, "fixture {index}: {response}");
                assert_eq!(response["error"]["name"], "Error", "fixture {index}");
                assert_eq!(
                    response["error"]["message"].as_str(),
                    Some(error.message()),
                    "fixture {index}"
                );
            }
        }
    }
}

#[test]
fn eval_inputs_serialize_to_the_typescript_shape() {
    use pi_coding_agent::utils::js_json;
    use pi_evals::pi_harness::{PiCodingAgentInput, PiCodingAgentStep};

    let prompt = PiCodingAgentInput::Prompt("hello".to_owned());
    assert_eq!(js_json::stringify(&serde_json::to_value(&prompt).unwrap()), "\"hello\"");
    let steps = PiCodingAgentInput::Steps(vec![
        PiCodingAgentStep::Prompt {
            content: "Create an extension.".to_owned(),
        },
        PiCodingAgentStep::Reload,
        PiCodingAgentStep::Prompt {
            content: "Use it.".to_owned(),
        },
    ]);
    let value: Value = serde_json::to_value(&steps).unwrap();
    assert_eq!(
        js_json::stringify(&value),
        "[{\"type\":\"prompt\",\"content\":\"Create an extension.\"},{\"type\":\"reload\"},{\"type\":\"prompt\",\"content\":\"Use it.\"}]"
    );
}
