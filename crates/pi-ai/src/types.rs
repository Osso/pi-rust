//! Port of the data contracts in `packages/ai/src/types.ts`.
//!
//! TypeScript `number` fields are `f64` serialized through [`pi_js_compat::number`];
//! Unix-millisecond timestamps are `i64`. Option/callback interfaces
//! (`StreamOptions` and friends) live with the provider runtime.

use indexmap::IndexMap;
use std::collections::BTreeMap;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::utils::diagnostics::AssistantMessageDiagnostic;
use pi_js_compat::number as js_number;

/// `Api`: a known API literal or any custom string.
pub type Api = String;
/// `ProviderId`: a known provider literal or any custom string.
pub type ProviderId = String;
pub type ImagesApi = String;
pub type ImagesProviderId = String;
pub type JsonObject = Map<String, Value>;

pub const KNOWN_APIS: [&str; 10] = [
    "openai-completions",
    "mistral-conversations",
    "openai-responses",
    "azure-openai-responses",
    "openai-codex-responses",
    "anthropic-messages",
    "bedrock-converse-stream",
    "google-generative-ai",
    "google-vertex",
    "pi-messages",
];

pub const KNOWN_IMAGES_APIS: [&str; 1] = ["openrouter-images"];

pub const KNOWN_PROVIDERS: [&str; 41] = [
    "amazon-bedrock",
    "ant-ling",
    "anthropic",
    "google",
    "google-vertex",
    "openai",
    "azure-openai-responses",
    "openai-codex",
    "openai-codex-gc",
    "radius",
    "nvidia",
    "deepseek",
    "github-copilot",
    "xai",
    "groq",
    "cerebras",
    "openrouter",
    "vercel-ai-gateway",
    "zai",
    "zai-coding-cn",
    "mistral",
    "minimax",
    "minimax-cn",
    "moonshotai",
    "moonshotai-cn",
    "huggingface",
    "fireworks",
    "together",
    "baseten",
    "opencode",
    "opencode-go",
    "kimi-coding",
    "cloudflare-workers-ai",
    "cloudflare-ai-gateway",
    "qwen-token-plan",
    "qwen-token-plan-cn",
    "qwen-token-plan-individual",
    "xiaomi",
    "xiaomi-token-plan-cn",
    "xiaomi-token-plan-ams",
    "xiaomi-token-plan-sgp",
];

pub const KNOWN_IMAGES_PROVIDERS: [&str; 1] = ["openrouter"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

/// `Partial<Record<ModelThinkingLevel, string | null>>`; `None` is an explicit `null`.
pub type ThinkingLevelMap = BTreeMap<ModelThinkingLevel, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatTemplateVarName {
    #[serde(rename = "thinking.enabled")]
    ThinkingEnabled,
    #[serde(rename = "thinking.effort")]
    ThinkingEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTemplateVar {
    #[serde(rename = "$var")]
    pub var: ChatTemplateVarName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omit_when_off: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatTemplateKwargValue {
    Var(ChatTemplateVar),
    String(String),
    Number(#[serde(serialize_with = "js_number::serialize")] f64),
    Bool(bool),
    Null,
}

/// Token budgets for each thinking level (token-based providers only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThinkingBudgets {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub minimal: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub low: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub medium: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub high: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheRetention {
    None,
    Short,
    Long,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    Sse,
    Websocket,
    WebsocketCached,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionAffinityFormat {
    Openai,
    OpenaiNosession,
    Openrouter,
}

/// `ProviderEnv`: provider-scoped environment overrides in TypeScript object key order.
pub type ProviderEnv = IndexMap<String, String>;
/// `ProviderHeaders`: `None` suppresses a default header of the same name.
///
/// Insertion order is the send order TypeScript produces from object key order.
pub type ProviderHeaders = IndexMap<String, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderNativeFormat {
    #[serde(rename = "openai.responses.input")]
    OpenAIResponsesInput,
}

/// Provider-scoped context retained across compaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderNativeContext {
    pub provider: ProviderId,
    pub api: Api,
    pub format: ProviderNativeFormat,
    pub value: Value,
}

/// Provider-internal retry or transport fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRetryEvent {
    #[serde(serialize_with = "js_number::serialize")]
    pub attempt: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub max_attempts: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub delay_ms: Option<f64>,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_transport: Option<Transport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResponse {
    #[serde(serialize_with = "js_number::serialize")]
    pub status: f64,
    pub headers: BTreeMap<String, String>,
}

macro_rules! literal_tag {
    ($name:ident, $literal:literal) => {
        #[doc = concat!("The literal `\"", $literal, "\"` discriminant.")]
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            #[default]
            #[serde(rename = $literal)]
            Tag,
        }
    };
}

literal_tag!(TextTag, "text");
literal_tag!(ThinkingTag, "thinking");
literal_tag!(ImageTag, "image");
literal_tag!(ToolCallTag, "toolCall");
literal_tag!(UserRole, "user");
literal_tag!(AssistantRole, "assistant");
literal_tag!(ToolResultRole, "toolResult");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPhase {
    Commentary,
    FinalAnswer,
}

/// `TextSignatureV1`, stored JSON-encoded in `TextContent::text_signature`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSignatureV1 {
    pub v: TextSignatureVersion,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<TextPhase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TextSignatureVersion;

impl TryFrom<u8> for TextSignatureVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err(format!("unsupported text signature version {value}"))
        }
    }
}

impl From<TextSignatureVersion> for u8 {
    fn from(_: TextSignatureVersion) -> Self {
        1
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub r#type: TextTag,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            r#type: TextTag::Tag,
            text: text.into(),
            text_signature: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingContent {
    pub r#type: ThinkingTag,
    pub thinking: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_signature: Option<String>,
    /// The thinking was redacted; the opaque payload is in `thinking_signature`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted: Option<bool>,
}

impl ThinkingContent {
    pub fn new(thinking: impl Into<String>) -> Self {
        Self {
            r#type: ThinkingTag::Tag,
            thinking: thinking.into(),
            thinking_signature: None,
            redacted: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub r#type: ImageTag,
    /// Base64-encoded image data.
    pub data: String,
    pub mime_type: String,
}

impl ImageContent {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            r#type: ImageTag::Tag,
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallExecution {
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub r#type: ToolCallTag,
    pub id: String,
    pub name: String,
    pub arguments: JsonObject,
    /// Application-owned execution metadata; provider adapters ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolCallExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: JsonObject) -> Self {
        Self {
            r#type: ToolCallTag::Tag,
            id: id.into(),
            name: name.into(),
            arguments,
            execution: None,
            thought_signature: None,
            namespace: None,
        }
    }
}

/// `TextContent | ImageContent` (user blocks, tool results, image I/O).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextOrImageContent {
    Text(TextContent),
    Image(ImageContent),
}

pub type UserContentBlock = TextOrImageContent;
pub type ToolResultContent = TextOrImageContent;
pub type ImagesInputContent = TextOrImageContent;
pub type ImagesOutputContent = TextOrImageContent;

/// `TextContent | ThinkingContent | ToolCall`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssistantContent {
    Text(TextContent),
    Thinking(ThinkingContent),
    ToolCall(ToolCall),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCost {
    #[serde(serialize_with = "js_number::serialize")]
    pub input: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub output: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_read: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_write: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub total: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(serialize_with = "js_number::serialize")]
    pub input: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub output: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_read: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_write: f64,
    /// Subset of `cache_write` written with 1h retention (Anthropic only).
    #[serde(
        rename = "cacheWrite1h",
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub cache_write_1h: Option<f64>,
    /// Reasoning tokens; a subset of `output`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub reasoning: Option<f64>,
    #[serde(serialize_with = "js_number::serialize")]
    pub total_tokens: f64,
    pub cost: UsageCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    Pending,
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
    Deferred,
}

/// `Extract<StopReason, "stop" | "length" | "toolUse" | "deferred">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DoneReason {
    Stop,
    Length,
    ToolUse,
    Deferred,
}

/// `Extract<StopReason, "aborted" | "error">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorReason {
    Aborted,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredHandle {
    pub provider: String,
    pub model_id: String,
    pub api: String,
    /// Provider token, such as a response id or batch id plus row id.
    pub id: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub expires_at: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub poll_after_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSource {
    Interactive,
    Rpc,
    Extension,
}

/// `string | (TextContent | ImageContent)[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<UserContentBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessage {
    pub role: UserRole,
    pub content: UserContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_source: Option<InputSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_native: Option<ProviderNativeContext>,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub role: AssistantRole,
    pub content: Vec<AssistantContent>,
    pub api: Api,
    pub provider: ProviderId,
    pub model: String,
    /// Concrete response model when it differs from the requested `model`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_generation_result: Option<ImageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<AssistantMessageDiagnostic>>,
    pub usage: Usage,
    pub stop_reason: StopReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred: Option<DeferredHandle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_turn: Option<bool>,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

impl AssistantMessage {
    /// Empty `pending` message with zero usage, the shape providers start streaming from.
    pub fn empty(
        api: impl Into<Api>,
        provider: impl Into<ProviderId>,
        model: impl Into<String>,
        timestamp: i64,
    ) -> Self {
        Self {
            role: AssistantRole::Tag,
            content: Vec::new(),
            api: api.into(),
            provider: provider.into(),
            model: model.into(),
            response_model: None,
            response_id: None,
            image_generation_result: None,
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Pending,
            deferred: None,
            error_message: None,
            raw_stop_reason: None,
            end_turn: None,
            timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultMessage {
    pub role: ToolResultRole,
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    /// Usage from the tool execution itself; not part of main context accounting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Names from `Context::tools` that became available after this result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_tool_names: Option<Vec<String>>,
    pub is_error: bool,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// `UserMessage | AssistantMessage | ToolResultMessage`, discriminated by each `role` tag.
#[serde(untagged)]
pub enum Message {
    User(UserMessage),
    Assistant(Box<AssistantMessage>),
    ToolResult(ToolResultMessage),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagesContext {
    pub input: Vec<ImagesInputContent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImagesStopReason {
    Stop,
    Error,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantImages {
    pub api: ImagesApi,
    pub provider: ImagesProviderId,
    pub model: String,
    pub output: Vec<ImagesOutputContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    pub stop_reason: ImagesStopReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrammarFormat {
    OpenaiLark,
    OpenaiRegex,
}

pub type GrammarVariants = BTreeMap<GrammarFormat, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JsonSchemaStrictness {
    Prefer,
    Require,
}

/// Provider-side constrained sampling for a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstrainedSamplingConfig {
    JsonSchema { strict: JsonSchemaStrictness },
    Grammar { variants: GrammarVariants },
}

/// `false | ConstrainedSamplingConfig`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstrainedSampling {
    Disabled,
    Config(ConstrainedSamplingConfig),
}

impl Serialize for ConstrainedSampling {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Disabled => serializer.serialize_bool(false),
            Self::Config(config) => config.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConstrainedSampling {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Bool(false) => Ok(Self::Disabled),
            other => serde_json::from_value(other)
                .map(Self::Config)
                .map_err(D::Error::custom),
        }
    }
}

/// A JSON Schema built with the TypeBox-equivalent builder (`Type.*` /
/// [`crate::utils::typebox_helpers::string_enum`]).
///
/// TypeScript TypeBox schemas carry a hidden `~kind` marker (and
/// `Symbol.for("TypeBox.Kind")` in older releases). `Tool::parameters` is a
/// plain `serde_json::Value`, which cannot hold hidden properties, so the
/// origin is recorded explicitly on [`Tool::typebox_kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBoxSchema(Value);

impl TypeBoxSchema {
    pub fn new(parameters: Value) -> Self {
        Self(parameters)
    }

    pub fn value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool arguments (TypeBox `TSchema` in TypeScript).
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constrained_sampling: Option<ConstrainedSampling>,
    /// Whether `parameters` was built with TypeBox. TypeScript's
    /// `Value.Convert` only converts schemas carrying the hidden `~kind`
    /// marker; plain JSON schemas (MCP/extension tools) skip that pass.
    #[serde(skip)]
    pub typebox_kind: bool,
}

impl Tool {
    /// Tool with a plain JSON Schema (MCP/extension origin).
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            constrained_sampling: None,
            typebox_kind: false,
        }
    }

    /// Tool whose schema was built with the TypeBox-equivalent builder.
    pub fn typebox(name: impl Into<String>, description: impl Into<String>, parameters: TypeBoxSchema) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: parameters.into_value(),
            constrained_sampling: None,
            typebox_kind: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
}

/// Event protocol for `AssistantMessageEventStream`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantMessageEvent {
    Start {
        partial: AssistantMessage,
    },
    TextStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    TextDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    ThinkingStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    ThinkingDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: String,
        partial: AssistantMessage,
    },
    #[serde(rename = "toolcall_start")]
    ToolCallStart {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        partial: AssistantMessage,
    },
    #[serde(rename = "toolcall_delta")]
    ToolCallDelta {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        delta: String,
        partial: AssistantMessage,
    },
    #[serde(rename = "toolcall_end")]
    ToolCallEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        #[serde(rename = "toolCall")]
        tool_call: ToolCall,
        partial: AssistantMessage,
    },
    Done {
        reason: DoneReason,
        message: AssistantMessage,
    },
    Error {
        reason: ErrorReason,
        error: AssistantMessage,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaxTokensField {
    #[serde(rename = "max_completion_tokens")]
    MaxCompletionTokens,
    #[serde(rename = "max_tokens")]
    MaxTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThinkingFormat {
    Openai,
    Openrouter,
    Deepseek,
    Together,
    Baseten,
    Zai,
    Qwen,
    ChatTemplate,
    QwenChatTemplate,
    StringThinking,
    AntLing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheControlFormat {
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeferredToolsMode {
    Kimi,
}

/// Compatibility settings for OpenAI-compatible completions APIs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAICompletionsCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_developer_role: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_effort: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_usage_in_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_finish_reason: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens_field: Option<MaxTokensField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_tool_result_name: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_assistant_after_tool_result: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_thinking_as_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_reasoning_content_on_assistant_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_format: Option<ThinkingFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<IndexMap<String, ChatTemplateKwargValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_args: Option<IndexMap<String, ChatTemplateKwargValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_router_routing: Option<OpenRouterRouting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vercel_gateway_routing: Option<VercelGatewayRouting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zai_tool_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_thinking_token_budget: Option<bool>,
    #[serde(rename = "supportsOpenAIGrammarTools", skip_serializing_if = "Option::is_none")]
    pub supports_openai_grammar_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_strict_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control_format: Option<CacheControlFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_session_affinity_headers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred_tools_mode: Option<DeferredToolsMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_affinity_format: Option<SessionAffinityFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_long_cache_retention: Option<bool>,
}

/// Compatibility settings for OpenAI Responses APIs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAIResponsesCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_developer_role: Option<bool>,
    /// Not declared by `OpenAIResponsesCompat` in the TypeScript source, but the
    /// generated `opencode` catalog carries it on an `openai-responses` model
    /// and the TS runtime preserves the property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_effort: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_affinity_format: Option<SessionAffinityFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_long_cache_retention: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_strict_mode: Option<bool>,
    #[serde(rename = "supportsOpenAIGrammarTools", skip_serializing_if = "Option::is_none")]
    pub supports_openai_grammar_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_additional_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_tool_search: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_explicit_prompt_cache_mode: Option<bool>,
}

/// Compatibility settings for Anthropic Messages-compatible APIs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnthropicMessagesCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_eager_tool_input_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_long_cache_retention: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_session_affinity_headers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_cache_control_on_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_temperature: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_adaptive_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_empty_signature: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_strict_tools: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_tool_references: Option<bool>,
}

/// Compatibility settings for Amazon Bedrock models.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BedrockCompat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_strict_mode: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataCollection {
    Deny,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingSortOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// `string | null | undefined`: outer `None` omits the key, inner `None` is `null`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_nullable"
    )]
    pub partition: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RoutingSort {
    Strategy(String),
    Options(RoutingSortOptions),
}

/// `number | string` price value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(#[serde(serialize_with = "js_number::serialize")] f64),
    String(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MaxPrice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<NumberOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<NumberOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<NumberOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<NumberOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<NumberOrString>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PercentileCutoffs {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub p50: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub p75: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub p90: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub p99: Option<f64>,
}

/// `number | { p50?, p75?, p90?, p99? }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PercentileThreshold {
    Number(#[serde(serialize_with = "js_number::serialize")] f64),
    Percentiles(PercentileCutoffs),
}

/// OpenRouter provider routing preferences (sent as the `provider` request field).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenRouterRouting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_fallbacks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_parameters: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_collection: Option<DataCollection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zdr: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforce_distillable_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantizations: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<RoutingSort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_price: Option<MaxPrice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_min_throughput: Option<PercentileThreshold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_max_latency: Option<PercentileThreshold>,
}

/// Vercel AI Gateway routing preferences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VercelGatewayRouting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostRates {
    #[serde(serialize_with = "js_number::serialize")]
    pub input: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub output: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_read: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cache_write: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostTier {
    #[serde(flatten)]
    pub rates: ModelCostRates,
    /// Applies when total input usage exceeds this token count.
    #[serde(serialize_with = "js_number::serialize")]
    pub input_tokens_above: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCost {
    #[serde(flatten)]
    pub rates: ModelCostRates,
    /// Request-wide pricing tiers; the highest matching input threshold applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers: Option<Vec<ModelCostTier>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Image,
}

/// `Model<TApi>["compat"]`, typed by the model's `api`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ModelCompat {
    OpenAICompletions(Box<OpenAICompletionsCompat>),
    OpenAIResponses(OpenAIResponsesCompat),
    AnthropicMessages(AnthropicMessagesCompat),
    Bedrock(BedrockCompat),
}

impl ModelCompat {
    /// Parses `compat` for `api`; custom APIs have no compat type (`never`).
    pub fn parse(api: &str, value: Value) -> Result<Option<Self>, serde_json::Error> {
        let compat = match api {
            "openai-completions" => Self::OpenAICompletions(Box::new(serde_json::from_value(value)?)),
            "openai-responses" | "azure-openai-responses" | "openai-codex-responses" => {
                Self::OpenAIResponses(serde_json::from_value(value)?)
            }
            "anthropic-messages" => Self::AnthropicMessages(serde_json::from_value(value)?),
            "bedrock-converse-stream" => Self::Bedrock(serde_json::from_value(value)?),
            _ => return Ok(None),
        };
        Ok(Some(compat))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub provider: ProviderId,
    pub base_url: String,
    pub reasoning: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<Modality>,
    pub cost: ModelCost,
    #[serde(serialize_with = "js_number::serialize")]
    pub context_window: f64,
    /// Exact context-token count at which automatic compaction begins.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub auto_compaction_threshold: Option<f64>,
    #[serde(serialize_with = "js_number::serialize")]
    pub max_tokens: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_params: Option<JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<IndexMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat: Option<ModelCompat>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelWire {
    id: String,
    name: String,
    api: Api,
    provider: ProviderId,
    base_url: String,
    reasoning: bool,
    thinking_level_map: Option<ThinkingLevelMap>,
    input: Vec<Modality>,
    cost: ModelCost,
    context_window: f64,
    auto_compaction_threshold: Option<f64>,
    max_tokens: f64,
    sampling_params: Option<JsonObject>,
    headers: Option<IndexMap<String, String>>,
    compat: Option<Value>,
}

impl<'de> Deserialize<'de> for Model {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ModelWire::deserialize(deserializer)?;
        let compat = match wire.compat {
            Some(value) => ModelCompat::parse(&wire.api, value).map_err(D::Error::custom)?,
            None => None,
        };
        Ok(Self {
            id: wire.id,
            name: wire.name,
            api: wire.api,
            provider: wire.provider,
            base_url: wire.base_url,
            reasoning: wire.reasoning,
            thinking_level_map: wire.thinking_level_map,
            input: wire.input,
            cost: wire.cost,
            context_window: wire.context_window,
            auto_compaction_threshold: wire.auto_compaction_threshold,
            max_tokens: wire.max_tokens,
            sampling_params: wire.sampling_params,
            headers: wire.headers,
            compat,
        })
    }
}

/// `ImagesModel<TApi>`: `Model` without the chat-only fields, plus `output` modalities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagesModel {
    pub id: String,
    pub name: String,
    pub api: ImagesApi,
    pub provider: ImagesProviderId,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<Modality>,
    pub output: Vec<Modality>,
    pub cost: ModelCost,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub auto_compaction_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_params: Option<JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<IndexMap<String, String>>,
}

fn deserialize_present_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}
