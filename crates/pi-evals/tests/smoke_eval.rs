//! Behavioral tests for `evals/src/smoke.eval.ts`.

#[path = "support/faux.rs"]
mod faux_support;

use pi_evals::evals::smoke::{EVAL_SET, create_smoke_harness};
use pi_evals::harness::HarnessContext;
use pi_evals::pi_harness::{
    ModelSelection, PiCodingAgentHarnessOptions, PiCodingAgentInput, PiModelEnvironment, resolve_model_selection,
};
use pi_js_compat::string::js_trim;

const PROMPT: &str = "What's the capital of France? Respond with only the city name.";

#[tokio::test]
async fn smoke_eval_runs_a_basic_prompt_end_to_end() {
    let fixture = faux_support::faux_runtime().await;
    fixture
        .faux
        .set_responses(vec![faux_support::terminal_response("Paris")]);
    let harness = create_smoke_harness(PiCodingAgentHarnessOptions {
        model: Some(ModelSelection {
            provider: "faux".to_owned(),
            id: "faux-1".to_owned(),
        }),
        model_runtime: Some(fixture.runtime),
        ..Default::default()
    });
    let run = harness
        .run(PiCodingAgentInput::Prompt(PROMPT.to_owned()), HarnessContext::new())
        .await
        .expect("smoke eval run");

    assert_eq!(run.output.as_str().map(js_trim), Some("Paris"));
    assert_eq!(run.errors, Vec::new());
    assert_eq!(run.usage.provider.as_deref(), Some("faux"));
    assert_eq!(run.usage.model.as_deref(), Some("faux-1"));
    assert!(run.usage.total_tokens.expect("total tokens") > 0.0);
    assert_eq!(EVAL_SET, "Pi Coding Agent smoke");
}

#[test]
fn smoke_eval_requires_a_model_selection() {
    let error = resolve_model_selection(None, &PiModelEnvironment::default())
        .expect_err("the eval cannot run without a model selection");
    assert_eq!(
        error.message(),
        "Select a harness model explicitly or set both PI_PROVIDER and PI_MODEL as defaults."
    );
}

#[tokio::test]
async fn smoke_eval_requires_a_live_model_when_opted_in() {
    let environment = PiModelEnvironment::from_process();
    if environment.pi_provider.is_none() || environment.pi_model.is_none() {
        eprintln!(
            "skipping live smoke eval: set PI_PROVIDER and PI_MODEL to opt in (matches the TypeScript runner gating)"
        );
        return;
    }
    let harness = create_smoke_harness(PiCodingAgentHarnessOptions::default());
    let run = harness
        .run(PiCodingAgentInput::Prompt(PROMPT.to_owned()), HarnessContext::new())
        .await
        .expect("live smoke eval run");
    assert_eq!(run.output.as_str().map(js_trim), Some("Paris"));
    assert_eq!(run.errors, Vec::new());
    assert_eq!(run.usage.provider, environment.pi_provider);
    assert_eq!(run.usage.model, environment.pi_model);
    assert!(run.usage.total_tokens.expect("total tokens") > 0.0);
}
