//! Bundled `codex-image-generation` extension
//! (`packages/coding-agent/extensions/codex-image-generation/src/index.ts`).

use super::hosted_deadline::{format_timeout, spawn_deadline};
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::compat::stream_simple;
use pi_ai::request::{BoxFuture, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    Context, ImageContent, Model, ProviderEnv, ProviderHeaders, TextContent, TextOrImageContent, UserContent,
    UserMessage, UserRole,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::node_fs::node_file_write_error_record;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue as Value, encode_node_utf8},
};
use serde_json::json as schema_json;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, ToolDefinition};
use crate::core::model_registry::ResolvedRequestAuth;
use crate::core::tools::node_fs::node_fs_error;
use crate::utils::paths::node_join;

const DEFAULT_IMAGE_GENERATION_TIMEOUT_MS: u64 = 300_000;

#[derive(Clone)]
pub struct ImageGenerationRequest {
    pub prompt: JsString,
    pub model: Model,
    pub api_key: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub env: Option<ProviderEnv>,
    pub signal: Option<AbortSignal>,
}

pub type RunImageGeneration = Arc<
    dyn Fn(ImageGenerationRequest, ExtensionContext) -> BoxFuture<'static, Result<ImageContent, JsString>>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct ImageGenerationToolOptions {
    pub run_generation: Option<RunImageGeneration>,
    pub timeout_ms: Option<u64>,
}

pub fn is_openai_hosted_image_generation_model(model: Option<&Model>) -> bool {
    model.is_some_and(|model| {
        model.api == "openai-codex-responses"
            && matches!(
                model.provider.as_str(),
                "openai-codex" | "openai-codex-gc" | "openai-codex-team"
            )
    })
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn hosted_payload_keeps_raw_content_and_tool_arguments() {
        let payload = pi_js_compat::js_json::parse_lossless(
            r#"{"input":"\ud800","tools":[{"type":"function","name":"keep","parameters":{"label":"\udc00"}}]}"#,
        )
        .unwrap();
        let changed = add_image_generation_tool_to_payload(&payload).unwrap();
        assert_eq!(changed["input"], payload["input"]);
        assert_eq!(changed["tools"][0], payload["tools"][0]);
    }
}

pub fn add_image_generation_tool_to_payload(payload: &Value) -> Option<Value> {
    let object = payload.as_object()?;
    let tools = object
        .get("tools")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tools_without_function: Vec<Value> = tools
        .iter()
        .filter(|tool| !is_function_image_generation_tool(tool))
        .cloned()
        .collect();
    if tools_without_function.iter().any(is_hosted_image_generation_tool) {
        if tools_without_function.len() == tools.len() {
            return None;
        }
        let mut updated = object.clone();
        updated.insert("tools".into(), Value::Array(tools_without_function));
        return Some(Value::Object(updated));
    }
    let mut updated_tools = tools_without_function;
    updated_tools.push(js_json!({"type": "image_generation", "output_format": "png"}));
    let mut updated = object.clone();
    updated.insert("tools".into(), Value::Array(updated_tools));
    Some(Value::Object(updated))
}

fn is_hosted_image_generation_tool(value: &Value) -> bool {
    value.get("type").and_then(Value::as_string) == Some(&JsString::from("image_generation"))
}

fn is_function_image_generation_tool(value: &Value) -> bool {
    value.get("type").and_then(Value::as_string) == Some(&JsString::from("function"))
        && value.get("name").and_then(Value::as_string) == Some(&JsString::from("image_gen"))
}

async fn execute_image_generation(
    params: &Value,
    ctx: &ExtensionContext,
    signal: Option<AbortSignal>,
    run_generation: &RunImageGeneration,
    timeout_ms: u64,
) -> Result<ImageContent, HandlerError> {
    let prompt = params
        .get("prompt")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default()
        .trim();
    if prompt.is_empty() {
        return Err(HandlerError::new("image_gen prompt is required"));
    }
    let Some(model) = ctx.model()? else {
        return Err(HandlerError::new("image_gen requires a Codex provider model"));
    };
    if !is_openai_hosted_image_generation_model(Some(&model)) {
        return Err(HandlerError::new("image_gen requires a Codex provider model"));
    }
    let auth = ctx
        .model_registry()?
        .get_api_key_and_headers(&model)
        .await
        .map_err(|error| HandlerError::new(error.message))?;
    let (api_key, headers, env) = match auth {
        ResolvedRequestAuth::Ok {
            api_key, headers, env, ..
        } => (api_key, headers, env),
        ResolvedRequestAuth::Err { error } => return Err(HandlerError::new(error)),
    };
    let message = format!(
        "OpenAI hosted image generation timed out after {}",
        format_timeout(timeout_ms)
    );
    let (combined, timer) = spawn_deadline(signal, timeout_ms, message.into());
    let result = run_generation(
        ImageGenerationRequest {
            prompt: prompt.to_owned(),
            model,
            api_key,
            headers,
            env,
            signal: Some(combined),
        },
        ctx.clone(),
    )
    .await;
    timer.abort();
    result.map_err(HandlerError::from)
}

fn save_generated_image(image: &ImageContent, cwd: &JsString) -> Result<JsString, HandlerError> {
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).expect("OS random source must be available");
    let name = random.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let output_path = node_join(cwd.clone(), js_format!("image-gen-{}.png", name));
    let bytes = pi_js_compat::base64::buffer_from_base64(&image.data);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(encode_node_utf8(&output_path))
        .map_err(|error| HandlerError::new(node_fs_error(&error, "open", &output_path)))?;
    std::io::Write::write_all(&mut file, &bytes)
        .map_err(|error| HandlerError::new(node_file_write_error_record(&error).message))?;
    Ok(output_path)
}

async fn run_hosted_image_generation(request: ImageGenerationRequest) -> Result<ImageContent, JsString> {
    let context = Context {
        system_prompt: Some(
            "Generate the requested image. Use the hosted image generation tool and return the generated image."
                .to_owned()
                .into(),
        ),
        messages: vec![pi_ai::types::Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(request.prompt.clone().into()),
            input_source: None,
            provider_native: None,
            timestamp: pi_js_compat::date::js_now_millis(),
        })],
        tools: None,
    };
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                signal: request.signal.clone(),
                api_key: request.api_key.clone(),
                headers: request.headers.clone(),
                env: request.env.clone(),
                on_payload: Some(Arc::new(|payload, _model| {
                    Box::pin(async move { Ok(add_image_generation_tool_to_payload(&payload)) })
                })),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let stream = stream_simple(request.model, context, options);
    let message = stream
        .map_err(|error| error.message)?
        .result()
        .await
        .map_err(|failure| failure.message().to_owned())?;
    if matches!(
        message.stop_reason,
        pi_ai::types::StopReason::Error | pi_ai::types::StopReason::Aborted
    ) {
        return Err(message
            .error_message
            .unwrap_or_else(|| "OpenAI hosted image generation failed".into()));
    }
    message
        .image_generation_result
        .ok_or_else(|| "OpenAI hosted image generation returned no image".into())
}

pub fn create_image_generation_tool_definition(options: &ImageGenerationToolOptions) -> ToolDefinition {
    let run_generation = options
        .run_generation
        .clone()
        .unwrap_or_else(|| Arc::new(|request, _ctx| Box::pin(run_hosted_image_generation(request))));
    let timeout_ms = options.timeout_ms.unwrap_or(DEFAULT_IMAGE_GENERATION_TIMEOUT_MS);
    ToolDefinition {
        name: "image_gen".to_owned(),
        label: "Image Generation".into(),
        description: "Generate an image using the OpenAI Responses hosted image generation tool.".into(),
        prompt_snippet: Some("Generate an image from a detailed prompt.".into()),
        prompt_guidelines: Some(vec![
            "Use image_gen when the user asks to create a new image.".into(),
            "Describe composition, subject, style, lighting, palette, and constraints in the prompt.".into(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters: schema_json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Detailed description of the image to generate."
                }
            }
        }),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_tool_call_id, params, signal, _on_update, ctx| {
            let run_generation = run_generation.clone();
            Box::pin(async move {
                let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                let image = execute_image_generation(&params, &ctx, signal, &run_generation, timeout_ms).await?;
                let output_path = save_generated_image(&image, &ctx.cwd()?)?;
                Ok(AgentToolResult {
                    content: vec![
                        TextOrImageContent::Text(TextContent::new(js_format!("Generated image: {}", output_path))),
                        TextOrImageContent::Image(image),
                    ],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}

/// Bundled `codex-image-generation` extension.
pub fn codex_image_generation_extension(
    pi: &ExtensionApi,
    options: &ImageGenerationToolOptions,
) -> Result<(), HandlerError> {
    pi.register_tool(create_image_generation_tool_definition(options))
        .map_err(HandlerError::from)
}

/// Registers the extension factory under `<first-party:codex-image-generation>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry) {
    registry.register(
        "<first-party:codex-image-generation>",
        Arc::new(|pi| {
            let result = codex_image_generation_extension(pi, &ImageGenerationToolOptions::default());
            Box::pin(async move { result })
        }),
    );
}
