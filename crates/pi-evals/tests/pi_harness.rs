//! Behavioral tests for `evals/src/pi-harness.ts` against a faux provider.

use std::sync::Arc;

#[path = "support/faux.rs"]
mod faux_support;

use pi_ai::providers::faux::{FauxAssistantMessageOptions, faux_assistant_message, faux_tool_call};
use pi_ai::types::StopReason;
use pi_coding_agent::core::model_runtime::ModelRuntime;
use pi_coding_agent::core::sdk::NoTools;
use pi_evals::error::PiEvalError;
use pi_evals::harness::{HarnessContext, HarnessRun, ToolCallStatus, TranscriptEvent, TranscriptRole, tool_calls};
use pi_evals::pi_harness::{
    ModelSelection, PiCodingAgentHarnessOptions, PiCodingAgentInput, PiCodingAgentStep, PiHarnessOutputArgs,
    PiHarnessOutputFn, create_pi_coding_agent_harness,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

fn options(runtime: ModelRuntime) -> PiCodingAgentHarnessOptions {
    PiCodingAgentHarnessOptions {
        name: Some("pi-coding-agent".to_owned()),
        model: Some(ModelSelection {
            provider: "faux".to_owned(),
            id: "faux-1".to_owned(),
        }),
        no_tools: Some(NoTools::All),
        model_runtime: Some(runtime),
        ..Default::default()
    }
}

fn output_transform(transform: impl Fn(PiHarnessOutputArgs) -> Value + Send + Sync + 'static) -> PiHarnessOutputFn {
    Arc::new(move |args| {
        let output = transform(args);
        Box::pin(async move { Ok(output) })
    })
}

fn session_snapshot(run: &HarnessRun) -> String {
    run.artifacts
        .as_ref()
        .and_then(|artifacts| artifacts.get(pi_evals::vitest_evals::artifacts::PI_SESSION_SNAPSHOT_ARTIFACT))
        .and_then(Value::as_str)
        .expect("the harness must record the native session snapshot")
        .to_owned()
}

fn assistant_texts(run: &HarnessRun) -> Vec<String> {
    run.session
        .events
        .iter()
        .filter_map(|event| match event {
            TranscriptEvent::Message {
                role: TranscriptRole::Assistant,
                content: Some(Value::String(text)),
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn harness_runs_a_prompt_with_a_faux_provider() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![faux_support::terminal_response("Paris")]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    assert_eq!(harness.name, "pi-coding-agent");

    let run = harness
        .run(
            PiCodingAgentInput::Prompt("What's the capital of France? Respond with only the city name.".to_owned()),
            HarnessContext::new(),
        )
        .await
        .expect("prompt must succeed");

    assert_eq!(run.output, json!("Paris"));
    assert_eq!(run.errors, Vec::new());
    assert_eq!(assistant_texts(&run), vec!["Paris".to_owned()]);
    let usage = &run.usage;
    assert_eq!(usage.provider.as_deref(), Some("faux"));
    assert_eq!(usage.model.as_deref(), Some("faux-1"));
    assert!(usage.total_tokens.expect("total tokens") > 0.0);
    assert_eq!(usage.tool_calls, Some(1.0));
    let metadata = usage.metadata.as_ref().expect("usage metadata");
    assert!(metadata.get("cacheReadTokens").and_then(Value::as_f64).is_some());
    assert!(metadata.get("cacheWriteTokens").and_then(Value::as_f64).is_some());
    assert_eq!(metadata.get("estimatedCostUsd"), Some(&json!(0)));

    assert_eq!(
        run.session.events.first(),
        Some(&TranscriptEvent::Message {
            role: TranscriptRole::User,
            content: Some(json!("What's the capital of France? Respond with only the city name.")),
            metadata: None,
        })
    );

    let artifacts = run.artifacts.as_ref().expect("run artifacts");
    let run_id = artifacts.get("runId").and_then(Value::as_str).expect("run id");
    assert!(!run_id.is_empty());
    let snapshot = session_snapshot(&run);
    assert!(snapshot.contains("\"type\":\"session\""));
    assert!(snapshot.contains("\"role\":\"user\""));

    let timings = run.timings.as_ref().expect("timings");
    assert!(timings.total_ms.expect("total ms") >= 0.0);
}

#[tokio::test]
async fn harness_records_tool_calls_and_failed_tool_results() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![
        faux_assistant_message(
            vec![faux_tool_call(
                "hello",
                faux_support::tool_arguments(json!({"name": "Bob"})),
                Some("call-1".to_owned()),
            )],
            FauxAssistantMessageOptions {
                stop_reason: Some(StopReason::Stop),
                ..Default::default()
            },
        )
        .into(),
        faux_support::terminal_response("Hello, Bob!"),
    ]);
    let mut harness_options = options(runtime);
    harness_options.no_tools = None;
    let harness = create_pi_coding_agent_harness(harness_options);

    let run = harness
        .run(
            PiCodingAgentInput::Prompt("greet Bob".to_owned()),
            HarnessContext::new(),
        )
        .await
        .expect("prompt must succeed");

    assert_eq!(run.output, json!("Hello, Bob!"));
    let calls = tool_calls(&run);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "hello");
    assert_eq!(calls[0].status, ToolCallStatus::Error);
    assert_eq!(
        calls[0].error.as_ref().map(|error| error.message.as_str()),
        Some("Tool hello not found")
    );
    assert_eq!(calls[1].name, "end_turn");
    assert_eq!(calls[1].status, ToolCallStatus::Ok);
    assert_eq!(run.usage.tool_calls, Some(2.0));
    assert!(matches!(
        &run.session.events[1],
        TranscriptEvent::ToolCall { id, name, arguments, .. }
            if id == "call-1" && name == "hello" && arguments.as_ref() == json!({"name": "Bob"}).as_object()
    ));
    assert!(matches!(
        &run.session.events[2],
        TranscriptEvent::ToolResult { tool_call_id, name, error: Some(error), .. }
            if tool_call_id == "call-1" && name.as_deref() == Some("hello") && error.message == "Tool hello not found"
    ));
}

#[tokio::test]
async fn harness_runs_prompt_and_reload_steps_in_order() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![
        faux_support::terminal_response("first"),
        faux_support::terminal_response("second"),
    ]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    let run = harness
        .run(
            PiCodingAgentInput::Steps(vec![
                PiCodingAgentStep::Prompt {
                    content: "first prompt".to_owned(),
                },
                PiCodingAgentStep::Reload,
                PiCodingAgentStep::Prompt {
                    content: "second prompt".to_owned(),
                },
            ]),
            HarnessContext::new(),
        )
        .await
        .expect("steps must succeed");
    assert_eq!(run.output, json!("second"));
    assert_eq!(assistant_texts(&run), vec!["first".to_owned(), "second".to_owned()]);
}

#[tokio::test]
async fn harness_applies_the_system_prompt_transform() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![faux_support::terminal_response("done")]);
    let mut harness_options = options(runtime);
    harness_options.output = Some(output_transform(
        |args: PiHarnessOutputArgs| json!({ "systemPrompt": args.session.system_prompt(), "response": args.response }),
    ));
    harness_options.transform_system_prompt = Some(Arc::new(|prompt: &str| {
        let index = prompt
            .find("\nGuidelines:\n")
            .ok_or_else(|| PiEvalError::error("Default Pi system prompt has no Guidelines section."))?;
        Ok(prompt[..index].to_owned())
    }));
    let harness = create_pi_coding_agent_harness(harness_options);

    let run = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect("transformed prompt must succeed");
    let system_prompt = run.output["systemPrompt"].as_str().expect("system prompt");
    assert!(!system_prompt.contains("\nGuidelines:\n"));
    assert!(!system_prompt.contains("\nPi documentation (read only"));
    assert!(system_prompt.contains("\nCurrent working directory: "));
    assert_eq!(run.output["response"], json!("done"));
}

#[tokio::test]
async fn harness_rejects_an_empty_transformed_system_prompt() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![]);
    let mut harness_options = options(runtime);
    harness_options.transform_system_prompt = Some(Arc::new(|_prompt: &str| Ok("   ".to_owned())));
    let harness = create_pi_coding_agent_harness(harness_options);
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect_err("empty transformed prompts must fail");
    assert_eq!(error.message(), "Transformed eval system prompt must not be empty.");
}

#[tokio::test]
async fn harness_reports_a_missing_model() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![]);
    let mut harness_options = options(runtime);
    harness_options.model = Some(ModelSelection {
        provider: "faux".to_owned(),
        id: "missing-model".to_owned(),
    });
    let harness = create_pi_coding_agent_harness(harness_options);
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect_err("missing models must fail");
    assert_eq!(error.message(), "Eval model not found: faux/missing-model");
}

#[tokio::test]
async fn harness_reports_abort_before_start() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    let token = CancellationToken::new();
    token.cancel();
    let context = HarnessContext {
        artifacts: serde_json::Map::new(),
        signal: Some(token),
    };
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), context)
        .await
        .expect_err("aborted runs must fail");
    assert_eq!(error.name(), "AbortError");
    assert_eq!(error.message(), "This operation was aborted");
}

#[tokio::test]
async fn harness_requires_a_prompt_step() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    let error = harness
        .run(
            PiCodingAgentInput::Steps(vec![PiCodingAgentStep::Reload]),
            HarnessContext::new(),
        )
        .await
        .expect_err("reload-only inputs must fail");
    assert_eq!(error.message(), "Pi eval input must include at least one prompt step.");
}

#[tokio::test]
async fn harness_propagates_output_transform_errors() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![faux_support::terminal_response("done")]);
    let mut harness_options = options(runtime);
    harness_options.output = Some(Arc::new(|_args: PiHarnessOutputArgs| {
        Box::pin(async move { Err(PiEvalError::error("output transform failed")) })
    }));
    let harness = create_pi_coding_agent_harness(harness_options);
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect_err("output transform errors must propagate");
    assert_eq!(error.message(), "output transform failed");
}

#[tokio::test]
async fn harness_reports_unexpected_stop_reasons() {
    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![
        faux_assistant_message(
            "partial",
            FauxAssistantMessageOptions {
                stop_reason: Some(StopReason::Aborted),
                ..Default::default()
            },
        )
        .into(),
    ]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect_err("unexpected stop reasons must fail");
    assert_eq!(error.message(), "Agent run ended with unexpected stop reason: aborted.");

    let fixture = faux_support::faux_runtime().await;
    let (runtime, faux) = (fixture.runtime, fixture.faux);
    faux.set_responses(vec![
        faux_assistant_message(
            "partial",
            FauxAssistantMessageOptions {
                stop_reason: Some(StopReason::Error),
                error_message: Some("provider exploded".to_owned()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    let harness = create_pi_coding_agent_harness(options(runtime));
    let error = harness
        .run(PiCodingAgentInput::Prompt("hello".to_owned()), HarnessContext::new())
        .await
        .expect_err("provider errors must fail");
    assert_eq!(error.message(), "provider exploded");
}
