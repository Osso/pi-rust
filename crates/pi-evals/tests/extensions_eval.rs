//! Behavioral tests for `evals/src/extensions.eval.ts`.

#[path = "support/faux.rs"]
mod faux_support;

use pi_coding_agent::core::model_runtime::ModelRuntime;
use pi_evals::evals::extensions::{
    BASELINE_HARNESS, CANDIDATE_HARNESS, EVAL_SET, EXTENSION_AUTHORING_JUDGE, ExtensionAuthoringOutput,
    LoadedExtensionSummary, exclude_guidelines_and_documentation, extension_authoring_judge, extension_harness_table,
    prepare_default_prompt_override,
};
use pi_evals::harness::{HarnessContext, ToolCall, ToolCallStatus};
use pi_evals::pi_harness::{
    ModelSelection, PiCodingAgentHarnessOptions, PiCodingAgentInput, PiCodingAgentStep, create_pi_coding_agent_harness,
};
use serde_json::json;

fn output(source: Option<&str>) -> ExtensionAuthoringOutput {
    ExtensionAuthoringOutput {
        response: "Hello, Bob!".to_owned(),
        system_prompt_has_guidelines: true,
        system_prompt_has_pi_docs: true,
        extension_errors: Vec::new(),
        loaded_extensions: vec![LoadedExtensionSummary {
            path: "/tmp/workspace/.pi/extensions/hello.ts".to_owned(),
            tools: vec!["hello".to_owned()],
        }],
        extension_source: source.map(str::to_owned),
    }
}

fn successful_call() -> ToolCall {
    ToolCall {
        name: "hello".to_owned(),
        arguments: Some(faux_support::tool_arguments(json!({"name": "Bob"}))),
        status: ToolCallStatus::Ok,
        result: Some(json!("Hello, Bob!")),
        error: None,
    }
}

fn harness_options(runtime: ModelRuntime) -> PiCodingAgentHarnessOptions {
    PiCodingAgentHarnessOptions {
        model: Some(ModelSelection {
            provider: "faux".to_owned(),
            id: "faux-1".to_owned(),
        }),
        no_tools: None,
        model_runtime: Some(runtime),
        ..Default::default()
    }
}

const CANONICAL_SOURCE: &str = "import { defineTool } from \"@earendil-works/pi-coding-agent\";\n";

#[test]
fn judge_accepts_the_completed_extension_authoring_workflow() {
    let result = extension_authoring_judge(&output(Some(CANONICAL_SOURCE)), &[successful_call()]);
    assert_eq!(result.score, 1.0);
    assert_eq!(
        result.metadata.get("rationale"),
        Some(&json!("Extension authoring workflow completed."))
    );
}

#[test]
fn judge_reports_each_failure_in_source_order() {
    let cases: Vec<(ExtensionAuthoringOutput, Vec<ToolCall>, &str)> = vec![
        (
            output(None),
            vec![successful_call()],
            "generated extension source is unavailable",
        ),
        (
            output(Some("import { defineTool } from \"./local\";\n")),
            vec![successful_call()],
            "extension does not import the canonical @earendil-works/pi-coding-agent package",
        ),
        (
            output(Some(
                "import { defineTool } from \"@earendil-works/pi-coding-agent\";\nimport legacy from \"@mariozechner/pi\";\n",
            )),
            vec![successful_call()],
            "extension imports a legacy @mariozechner package",
        ),
        (
            output(Some(
                "import { defineTool } from \"@earendil-works/pi-coding-agent\";\nimport { Type } from \"@sinclair/typebox\";\n",
            )),
            vec![successful_call()],
            "extension imports legacy \"@sinclair/typebox\" instead of \"typebox\"",
        ),
        (
            {
                let mut output = output(Some(CANONICAL_SOURCE));
                output.extension_errors = vec![pi_coding_agent::core::extensions::types::ExtensionLoadError {
                    path: "/tmp/hello.ts".to_owned(),
                    error: "load failed".to_owned(),
                }];
                output
            },
            vec![successful_call()],
            "extension loader reported errors",
        ),
        (
            {
                let mut output = output(Some(CANONICAL_SOURCE));
                output.loaded_extensions[0].tools = vec!["other".to_owned()];
                output
            },
            vec![successful_call()],
            "no loaded extension registered the \"hello\" tool",
        ),
        (
            output(Some(CANONICAL_SOURCE)),
            vec![ToolCall {
                status: ToolCallStatus::Pending,
                ..successful_call()
            }],
            "no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\"",
        ),
        (
            output(Some(CANONICAL_SOURCE)),
            vec![ToolCall {
                arguments: Some(faux_support::tool_arguments(json!({"name": "Alice"}))),
                ..successful_call()
            }],
            "no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\"",
        ),
        (
            output(Some(CANONICAL_SOURCE)),
            vec![ToolCall {
                result: Some(json!("Hello, Alice!")),
                ..successful_call()
            }],
            "no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\"",
        ),
        (
            {
                let mut output = output(Some(CANONICAL_SOURCE));
                output.response = "Hello, Alice!".to_owned();
                output
            },
            vec![successful_call()],
            "final response was not exactly \"Hello, Bob!\"",
        ),
    ];
    for (output, calls, expected) in cases {
        let result = extension_authoring_judge(&output, &calls);
        assert_eq!(result.score, 0.0, "{expected}");
        assert_eq!(result.metadata.get("rationale"), Some(&json!(expected)));
    }
}

#[test]
fn judge_joins_multiple_failures_with_semicolons() {
    let mut output = output(None);
    output.loaded_extensions.clear();
    let result = extension_authoring_judge(&output, &[]);
    assert_eq!(result.score, 0.0);
    assert_eq!(
        result.metadata.get("rationale"),
        Some(&json!(
            "generated extension source is unavailable; no loaded extension registered the \"hello\" tool; no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\""
        ))
    );
}

#[test]
fn system_prompt_transforms_match_the_source() {
    assert_eq!(
        exclude_guidelines_and_documentation("prefix\nGuidelines:\n- rule\nsuffix").expect("guidelines"),
        "prefix"
    );
    assert_eq!(
        exclude_guidelines_and_documentation("no guidelines")
            .expect_err("missing guidelines")
            .message(),
        "Default Pi system prompt has no Guidelines section."
    );
    assert_eq!(
        prepare_default_prompt_override(
            "prompt\nCurrent working directory: /first\nCurrent working directory: /last\n"
        )
        .expect("cwd"),
        "prompt\nCurrent working directory: /first"
    );
    assert_eq!(
        prepare_default_prompt_override("no cwd")
            .expect_err("missing cwd")
            .message(),
        "Default Pi system prompt has no working-directory section."
    );
}

#[tokio::test]
async fn harness_table_runs_the_extension_authoring_eval() {
    let baseline_fixture = faux_support::faux_runtime().await;
    let candidate_fixture = faux_support::faux_runtime().await;
    let rows = extension_harness_table(
        harness_options(baseline_fixture.runtime),
        harness_options(candidate_fixture.runtime),
    )
    .expect("harness table");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, BASELINE_HARNESS);
    assert_eq!(rows[1].name, CANDIDATE_HARNESS);
    assert_eq!(rows[0].repetition, 1);
    assert_eq!(rows[1].repetition, 1);
    assert_eq!(EVAL_SET, "Pi extension authoring system prompt");
    assert_eq!(EXTENSION_AUTHORING_JUDGE, "ExtensionAuthoringJudge");

    let steps = vec![
        PiCodingAgentStep::Prompt {
            content: "Create a Pi extension with a hello tool.".to_owned(),
        },
        PiCodingAgentStep::Reload,
        PiCodingAgentStep::Prompt {
            content: "Use the hello tool to greet Bob.".to_owned(),
        },
    ];
    let mut run_ids = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let faux = if index == 0 {
            &baseline_fixture.faux
        } else {
            &candidate_fixture.faux
        };
        faux.set_responses(vec![
            faux_support::terminal_response("created"),
            faux_support::terminal_response("Hello, Bob!"),
        ]);
        let run = row
            .harness
            .run(PiCodingAgentInput::Steps(steps.clone()), HarnessContext::new())
            .await
            .expect("eval run");
        let output: ExtensionAuthoringOutput = serde_json::from_value(run.output.clone()).expect("extension output");
        let expects_full_prompt = row.name == CANDIDATE_HARNESS;
        assert_eq!(output.system_prompt_has_guidelines, expects_full_prompt);
        assert_eq!(output.system_prompt_has_pi_docs, expects_full_prompt);
        assert_eq!(output.response, "Hello, Bob!");
        assert!(output.extension_source.is_none());
        assert!(output.loaded_extensions.is_empty());
        assert!(output.extension_errors.is_empty());

        let artifacts = run.artifacts.as_ref().expect("run artifacts");
        let run_id = artifacts.get("runId").and_then(|value| value.as_str());
        assert!(run_id.is_some());
        run_ids.push(run_id.expect("run id").to_owned());
        let iteration = artifacts
            .get("vitestEvalsHarnessIteration")
            .expect("iteration artifact");
        assert_eq!(iteration["evalSet"], json!(EVAL_SET));
        assert_eq!(iteration["harness"], json!(row.name));
        assert_eq!(iteration["baseline"], json!(BASELINE_HARNESS));
        assert_eq!(iteration["candidates"], json!([CANDIDATE_HARNESS]));
        assert_eq!(iteration["repetition"], json!(1));
        assert_eq!(
            iteration["groupKey"],
            pi_evals::vitest_evals::harness_table::derive_eval_group_key(
                &serde_json::to_value(PiCodingAgentInput::Steps(steps.clone())).expect("input"),
                1.0
            )
            .expect("group key")
        );

        let judge = extension_authoring_judge(&output, &[]);
        assert_eq!(judge.score, 0.0);
        assert_eq!(
            judge.metadata.get("rationale"),
            Some(&json!(
                "generated extension source is unavailable; no loaded extension registered the \"hello\" tool; no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\""
            ))
        );
    }
    assert_ne!(run_ids[0], run_ids[1]);
}

#[tokio::test]
async fn extension_authoring_eval_requires_a_live_model_when_opted_in() {
    let environment = pi_evals::pi_harness::PiModelEnvironment::from_process();
    if environment.pi_provider.is_none() || environment.pi_model.is_none() {
        eprintln!(
            "skipping live extension-authoring eval: set PI_PROVIDER and PI_MODEL to opt in (matches the TypeScript runner gating)"
        );
        return;
    }
    let harness = create_pi_coding_agent_harness(PiCodingAgentHarnessOptions {
        name: Some(CANDIDATE_HARNESS.to_owned()),
        transform_system_prompt: Some(std::sync::Arc::new(|prompt: &str| {
            prepare_default_prompt_override(prompt)
        })),
        ..Default::default()
    });
    let run = harness
        .run(
            PiCodingAgentInput::Steps(vec![
                PiCodingAgentStep::Prompt {
                    content: "Create a Pi extension with a hello tool that takes a name and returns a greeting. For example, passing Bob should return `Hello, Bob!`.".to_owned(),
                },
                PiCodingAgentStep::Reload,
                PiCodingAgentStep::Prompt {
                    content: "Use the hello tool to greet Bob. Respond with exactly the tool's greeting and nothing else.".to_owned(),
                },
            ]),
            HarnessContext::new(),
        )
        .await
        .expect("live eval run");
    let output: ExtensionAuthoringOutput = serde_json::from_value(run.output).expect("extension output");
    assert!(output.system_prompt_has_guidelines);
    assert!(output.system_prompt_has_pi_docs);
}
