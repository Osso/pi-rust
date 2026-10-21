//! Port of `packages/evals/src/extensions.eval.ts`.
//!
//! The eval case itself runs as a Rust integration test; this module keeps the
//! harness factory, system-prompt transforms, and judge the source file
//! defines.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use pi_coding_agent::core::extensions::types::ExtensionLoadError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::PiEvalError;
use crate::harness::{ToolCall, ToolCallStatus};
use crate::pi_harness::{
    PiCodingAgentHarness, PiCodingAgentHarnessOptions, PiCodingAgentInput, PiHarnessOutputArgs,
    PiSystemPromptTransform, create_pi_coding_agent_harness,
};
use crate::vitest_evals::harness_table::{
    EvalHarnessTableOptions, EvalHarnessTablePairOptions, EvalHarnessTableRow, eval_harness_table,
};

pub const EVAL_SET: &str = "Pi extension authoring system prompt";
pub const BASELINE_HARNESS: &str = "system-prompt-without-docs";
pub const CANDIDATE_HARNESS: &str = "default-system-prompt";
pub const EXTENSION_AUTHORING_JUDGE: &str = "ExtensionAuthoringJudge";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedExtensionSummary {
    pub path: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionAuthoringOutput {
    pub response: String,
    pub system_prompt_has_guidelines: bool,
    pub system_prompt_has_pi_docs: bool,
    pub extension_errors: Vec<ExtensionLoadError>,
    pub loaded_extensions: Vec<LoadedExtensionSummary>,
    pub extension_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JudgeResult {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub score: f64,
    pub metadata: Map<String, Value>,
}

async fn extension_authoring_output(args: PiHarnessOutputArgs) -> Result<Value, PiEvalError> {
    let extensions = args.session.resource_loader().get_extensions();
    let cwd = args.session.session_manager().get_cwd().to_owned();
    let extension_path = Path::new(&cwd).join(".pi").join("extensions").join("hello.ts");
    let extension_source = if extension_path.exists() {
        Some(
            tokio::fs::read_to_string(&extension_path)
                .await
                .map_err(|error| PiEvalError::error(error.to_string()))?,
        )
    } else {
        None
    };
    let output = ExtensionAuthoringOutput {
        response: args.response,
        system_prompt_has_guidelines: args.session.system_prompt().contains("\nGuidelines:\n"),
        system_prompt_has_pi_docs: args.session.system_prompt().contains("\nPi documentation (read only"),
        extension_errors: extensions.errors,
        loaded_extensions: extensions
            .extensions
            .iter()
            .map(|extension| {
                let extension = extension.read();
                LoadedExtensionSummary {
                    path: extension.path.clone(),
                    tools: extension.tools.keys().cloned().collect(),
                }
            })
            .collect(),
        extension_source,
    };
    serde_json::to_value(output).map_err(|error| PiEvalError::error(error.to_string()))
}

/// `createExtensionAuthoringHarness(name, transformSystemPrompt?)`.
///
/// `base` is the Rust carrier for the model/runtime options the TypeScript
/// source takes from the runner default.
pub fn create_extension_authoring_harness(
    name: impl Into<String>,
    transform_system_prompt: Option<PiSystemPromptTransform>,
    mut base: PiCodingAgentHarnessOptions,
) -> PiCodingAgentHarness {
    base.name = Some(name.into());
    base.transform_system_prompt = transform_system_prompt;
    base.output = Some(Arc::new(|args| Box::pin(extension_authoring_output(args))));
    create_pi_coding_agent_harness(base)
}

/// `excludeGuidelinesAndDocumentation`.
pub fn exclude_guidelines_and_documentation(default_prompt: &str) -> Result<String, PiEvalError> {
    let guidelines_start = default_prompt
        .find("\nGuidelines:\n")
        .ok_or_else(|| PiEvalError::error("Default Pi system prompt has no Guidelines section."))?;
    Ok(default_prompt[..guidelines_start].to_owned())
}

/// `prepareDefaultPromptOverride`.
pub fn prepare_default_prompt_override(default_prompt: &str) -> Result<String, PiEvalError> {
    let cwd_start = default_prompt
        .rfind("\nCurrent working directory: ")
        .ok_or_else(|| PiEvalError::error("Default Pi system prompt has no working-directory section."))?;
    Ok(default_prompt[..cwd_start].to_owned())
}

fn extension_import_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"\b(?:from|import)\s+["']([^"']+)["']"#).expect("extension import regex must compile")
    })
}

/// `ExtensionAuthoringJudge`.
pub fn extension_authoring_judge(output: &ExtensionAuthoringOutput, tool_calls: &[ToolCall]) -> JudgeResult {
    let mut failures = Vec::new();
    match output.extension_source.as_deref() {
        None => failures.push("generated extension source is unavailable"),
        Some(source) => {
            let imports: Vec<&str> = extension_import_regex()
                .captures_iter(source)
                .filter_map(|captures| captures.get(1).map(|capture| capture.as_str()))
                .collect();
            if !imports.contains(&"@earendil-works/pi-coding-agent") {
                failures.push("extension does not import the canonical @earendil-works/pi-coding-agent package");
            }
            if imports.iter().any(|specifier| specifier.starts_with("@mariozechner/")) {
                failures.push("extension imports a legacy @mariozechner package");
            }
            if imports
                .iter()
                .any(|specifier| specifier.starts_with("@sinclair/typebox"))
            {
                failures.push("extension imports legacy \"@sinclair/typebox\" instead of \"typebox\"");
            }
        }
    }
    if !output.extension_errors.is_empty() {
        failures.push("extension loader reported errors");
    }
    if !output
        .loaded_extensions
        .iter()
        .any(|extension| extension.tools.iter().any(|tool| tool == "hello"))
    {
        failures.push("no loaded extension registered the \"hello\" tool");
    }
    let hello_call_succeeded = tool_calls.iter().any(|call| {
        call.name == "hello"
            && call.status == ToolCallStatus::Ok
            && call
                .arguments
                .as_ref()
                .and_then(|arguments| arguments.get("name"))
                .and_then(Value::as_str)
                == Some("Bob")
            && call.result.as_ref().and_then(Value::as_str) == Some("Hello, Bob!")
    });
    if !hello_call_succeeded {
        failures.push("no successful hello({ name: \"Bob\" }) call returned \"Hello, Bob!\"");
    }
    if output.response != "Hello, Bob!" {
        failures.push("final response was not exactly \"Hello, Bob!\"");
    }

    let mut metadata = Map::new();
    metadata.insert(
        "rationale".to_owned(),
        Value::String(if failures.is_empty() {
            "Extension authoring workflow completed.".to_owned()
        } else {
            failures.join("; ")
        }),
    );
    JudgeResult {
        score: if failures.is_empty() { 1.0 } else { 0.0 },
        metadata,
    }
}

/// `evalHarnessTable("Pi extension authoring system prompt", { baseline, candidate })`.
pub fn extension_harness_table(
    baseline_options: PiCodingAgentHarnessOptions,
    candidate_options: PiCodingAgentHarnessOptions,
) -> Result<Vec<EvalHarnessTableRow<PiCodingAgentInput>>, PiEvalError> {
    let baseline_transform: PiSystemPromptTransform =
        Arc::new(|prompt: &str| exclude_guidelines_and_documentation(prompt));
    let candidate_transform: PiSystemPromptTransform = Arc::new(|prompt: &str| prepare_default_prompt_override(prompt));
    let baseline =
        create_extension_authoring_harness(BASELINE_HARNESS, Some(baseline_transform), baseline_options).into_harness();
    let candidate = create_extension_authoring_harness(CANDIDATE_HARNESS, Some(candidate_transform), candidate_options)
        .into_harness();
    eval_harness_table(
        EVAL_SET,
        EvalHarnessTableOptions::Pair(EvalHarnessTablePairOptions {
            baseline,
            candidate,
            repetitions: None,
        }),
    )
}
