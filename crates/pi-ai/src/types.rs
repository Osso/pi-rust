//! Port of the data contracts in `packages/ai/src/types.ts`.
//!
//! TypeScript `number` fields are `f64` serialized through [`pi_js_compat::number`];
//! Message timestamps retain JavaScript fractional/nonfinite numbers. Option/callback interfaces
//! (`StreamOptions` and friends) live with the provider runtime.

use indexmap::IndexMap;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::utils::diagnostics::AssistantMessageDiagnostic;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use pi_js_compat::number as js_number;

mod model_properties;
pub use model_properties::ModelProperties;

fn serialize_message_timestamp<S: Serializer>(timestamp: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    // Reuse the lossless JSON writer for exact Node number spelling and nonfinite nulls.
    JsValue::Number(*timestamp).serialize(serializer)
}

/// `Api`: a known API literal or any custom string.
pub type Api = String;
/// `ProviderId`: a known provider literal or any custom string.
pub type ProviderId = String;
pub type ImagesApi = String;
pub type ImagesProviderId = String;
pub type JsonObject = Map<String, Value>;

pub const KNOWN_APIS: [&str; 9] = [
    "openai-completions",
    "mistral-conversations",
    "openai-responses",
    "azure-openai-responses",
    "openai-codex-responses",
    "anthropic-messages",
    "bedrock-converse-stream",
    "google-generative-ai",
    "google-vertex",
];

pub const KNOWN_IMAGES_APIS: [&str; 1] = ["openrouter-images"];

pub const KNOWN_PROVIDERS: [&str; 36] = [
    "amazon-bedrock",
    "ant-ling",
    "anthropic",
    "google",
    "google-vertex",
    "openai",
    "openai-codex",
    "openai-codex-gc",
    "openai-codex-team",
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
    "opencode",
    "opencode-go",
    "kimi-coding",
    "cloudflare-workers-ai",
    "cloudflare-ai-gateway",
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

mod thinking_level_map;
pub use thinking_level_map::ThinkingLevelMap;

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

/// `ProviderEnv`: provider-scoped environment overrides. Variable names are
/// opaque scalar names; values keep exact UTF-16 until an OS/HTTP boundary.
pub type ProviderEnv = IndexMap<String, JsString>;
/// `ProviderHeaders`: `None` suppresses a default header of the same name.
/// Header names are opaque scalar names; values reach the wire through
/// `request::header_value`.
pub type ProviderHeaders = IndexMap<String, Option<JsString>>;

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
    pub value: JsValue,
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
    pub reason: pi_js_compat::js_json::JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_transport: Option<Transport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResponse {
    #[serde(serialize_with = "js_number::serialize")]
    pub status: f64,
    pub headers: IndexMap<String, String>,
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
    pub text: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_signature: Option<String>,
    /// Provider block index while streaming; absent after finalization.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub index: Option<f64>,
}

impl TextContent {
    /// Checked conversion for consumers that require Unicode scalar text.
    pub fn text_utf8(&self) -> Result<String, std::string::FromUtf16Error> {
        self.text.to_utf8()
    }

    pub fn new(text: impl Into<JsString>) -> Self {
        Self {
            r#type: TextTag::Tag,
            text: text.into(),
            text_signature: None,
            index: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingContent {
    pub r#type: ThinkingTag,
    pub thinking: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_signature: Option<String>,
    /// The thinking was redacted; the opaque payload is in `thinking_signature`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted: Option<bool>,
    /// Provider block index while streaming; absent after finalization.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub index: Option<f64>,
}

impl ThinkingContent {
    pub fn new(thinking: impl Into<JsString>) -> Self {
        Self {
            r#type: ThinkingTag::Tag,
            thinking: thinking.into(),
            thinking_signature: None,
            redacted: None,
            index: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    pub r#type: ImageTag,
    /// Base64-encoded image data.
    pub data: String,
    pub mime_type: String,
    /// Unknown message properties and original order; never provider request fields.
    pub properties: ModelProperties,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageContentWire {
    r#type: ImageTag,
    data: String,
    mime_type: String,
}

impl Serialize for ImageContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(
            &ImageContentWire {
                r#type: self.r#type,
                data: self.data.clone(),
                mime_type: self.mime_type.clone(),
            },
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ImageContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let (wire, properties): (ImageContentWire, _) =
            ModelProperties::decode(value, &["type", "data", "mimeType"]).map_err(D::Error::custom)?;
        Ok(Self {
            r#type: wire.r#type,
            data: wire.data,
            mime_type: wire.mime_type,
            properties,
        })
    }
}

impl ImageContent {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            r#type: ImageTag::Tag,
            data: data.into(),
            mime_type: mime_type.into(),
            properties: ModelProperties::default(),
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
    #[serde(
        serialize_with = "pi_js_compat::js_json::serialize_object",
        deserialize_with = "pi_js_compat::js_json::deserialize_object"
    )]
    pub arguments: JsObject,
    /// Application-owned execution metadata; provider adapters ignore it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolCallExecution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Provider block index while streaming; absent after finalization.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub index: Option<f64>,
    /// Unparsed tool input accumulated during streaming.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_json: Option<JsString>,
    /// OpenAI Completions argument buffer, retained in streaming snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_args: Option<JsString>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub stream_index: Option<f64>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: JsObject) -> Self {
        Self {
            r#type: ToolCallTag::Tag,
            id: id.into(),
            name: name.into(),
            arguments,
            execution: None,
            thought_signature: None,
            namespace: None,
            index: None,
            partial_json: None,
            partial_args: None,
            stream_index: None,
        }
    }
}

/// `TextContent | ImageContent` (user blocks, tool results, image I/O).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TextOrImageContent {
    Text(TextContent),
    Image(ImageContent),
}

// Serde's untagged buffering parses strings before selecting a variant, rejecting
// lone surrogates. Select on the lossless AST, then decode the selected record.
macro_rules! deserialize_content_union {
    ($name:ident, $key:literal, $( $tag:literal => $variant:ident ),+ $(,)?) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let value = JsValue::deserialize(d)?;
                let tag = value.get($key).and_then(JsValue::as_string)
                    .and_then(|text| text.to_utf8().ok());
                match tag.as_deref() {
                    $(Some($tag) => pi_js_compat::js_json::from_value(&value).map(Self::$variant).map_err(D::Error::custom),)+
                    _ => Err(D::Error::custom(concat!("invalid ", $key, " discriminator"))),
                }
            }
        }
    };
}
deserialize_content_union!(TextOrImageContent, "type", "text" => Text, "image" => Image);
deserialize_content_union!(AssistantContent, "type", "text" => Text, "thinking" => Thinking, "toolCall" => ToolCall);
deserialize_content_union!(Message, "role", "user" => User, "assistant" => Assistant, "toolResult" => ToolResult);

impl<'de> Deserialize<'de> for UserContent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(d)?;
        match value {
            JsValue::String(text) => Ok(Self::Text(text)),
            JsValue::Array(_) => pi_js_compat::js_json::from_value(&value)
                .map(Self::Blocks)
                .map_err(D::Error::custom),
            _ => Err(D::Error::custom("expected string or content array")),
        }
    }
}

pub type UserContentBlock = TextOrImageContent;
pub type ToolResultContent = TextOrImageContent;
pub type ImagesInputContent = TextOrImageContent;
pub type ImagesOutputContent = TextOrImageContent;

/// `TextContent | ThinkingContent | ToolCall`.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

/// Nominal done reasons plus classified custom-Anthropic/Codex `done/error` source discrepancies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DoneReason {
    Stop,
    Length,
    ToolUse,
    Error,
}

/// `Extract<StopReason, "aborted" | "error">`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorReason {
    Aborted,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSource {
    Interactive,
    Rpc,
    Extension,
}

/// `string | (TextContent | ImageContent)[]`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(JsString),
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
    /// Unix timestamp in milliseconds, retaining JavaScript fractional/nonfinite numbers.
    #[serde(serialize_with = "serialize_message_timestamp")]
    pub timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessage {
    pub role: AssistantRole,
    pub content: Vec<AssistantContent>,
    pub api: Api,
    pub provider: ProviderId,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: StopReason,
    /// Unix timestamp in milliseconds, retaining JavaScript fractional/nonfinite numbers.
    #[serde(serialize_with = "serialize_message_timestamp")]
    pub timestamp: f64,
    /// Concrete response model when it differs from the requested `model`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_generation_result: Option<ImageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<Vec<AssistantMessageDiagnostic>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<pi_js_compat::js_json::JsString>,
}

impl AssistantMessage {
    /// Empty message with zero usage and the frozen providers' initial `stop` reason.
    pub fn empty(
        api: impl Into<Api>,
        provider: impl Into<ProviderId>,
        model: impl Into<String>,
        timestamp: f64,
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
            stop_reason: StopReason::Stop,
            error_message: None,
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    pub is_error: bool,
    /// Unix timestamp in milliseconds, retaining JavaScript fractional/nonfinite numbers.
    #[serde(serialize_with = "serialize_message_timestamp")]
    pub timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub error_message: Option<pi_js_compat::js_json::JsString>,
    /// Unix timestamp in milliseconds, retaining JavaScript fractional numbers.
    #[serde(serialize_with = "serialize_message_timestamp")]
    pub timestamp: f64,
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
    /// Enumerable AgentTool label; absent on plain AI Tool values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
    pub description: JsString,
    /// JSON Schema for the tool arguments (TypeBox `TSchema` in TypeScript).
    pub parameters: Value,
    /// Enumerable AgentTool override; AI adapters do not interpret it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_mode: Option<JsString>,
    /// Whether `parameters` was built with TypeBox. TypeScript's
    /// `Value.Convert` only converts schemas carrying the hidden `~kind`
    /// marker; plain JSON schemas (MCP/extension tools) skip that pass.
    #[serde(skip)]
    pub typebox_kind: bool,
}

impl Tool {
    /// Tool with a plain JSON Schema (MCP/extension origin).
    pub fn new(name: impl Into<String>, description: impl Into<JsString>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            label: None,
            description: description.into(),
            parameters,
            execution_mode: None,
            typebox_kind: false,
        }
    }

    /// Tool whose schema was built with the TypeBox-equivalent builder.
    pub fn typebox(name: impl Into<String>, description: impl Into<JsString>, parameters: TypeBoxSchema) -> Self {
        Self {
            name: name.into(),
            label: None,
            description: description.into(),
            parameters: parameters.into_value(),
            execution_mode: None,
            typebox_kind: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<JsString>,
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
        delta: JsString,
        partial: AssistantMessage,
    },
    TextEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: JsString,
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
        delta: JsString,
        partial: AssistantMessage,
    },
    ThinkingEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        content: JsString,
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
        delta: JsString,
        partial: AssistantMessage,
    },
    #[serde(rename = "toolcall_end")]
    ToolCallEnd {
        #[serde(rename = "contentIndex")]
        content_index: usize,
        #[serde(rename = "toolCall")]
        tool_call: Box<ToolCall>,
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

mod compat;
pub use compat::{
    AnthropicMessagesCompat, CompatFields, OpenAICompletionsCompat, OpenAIResponsesCompat, spread_compat_properties,
};

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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelCostTier {
    pub rates: ModelCostRates,
    /// Applies when total input usage exceeds this token count.
    pub input_tokens_above: f64,
    pub properties: ModelProperties,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelCost {
    pub rates: ModelCostRates,
    /// Request-wide pricing tiers; the highest matching input threshold applies.
    pub tiers: Option<Vec<ModelCostTier>>,
    pub properties: ModelProperties,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelCostTierWire {
    #[serde(flatten)]
    rates: ModelCostRates,
    #[serde(serialize_with = "js_number::serialize")]
    input_tokens_above: f64,
}

#[derive(Deserialize, Serialize)]
struct ModelCostWire {
    #[serde(flatten)]
    rates: ModelCostRates,
    #[serde(skip_serializing_if = "Option::is_none")]
    tiers: Option<Vec<ModelCostTier>>,
}

impl Serialize for ModelCostTier {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(
            &ModelCostTierWire {
                rates: self.rates.clone(),
                input_tokens_above: self.input_tokens_above,
            },
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ModelCostTier {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let (wire, properties): (ModelCostTierWire, _) = ModelProperties::decode(
            value,
            &["input", "output", "cacheRead", "cacheWrite", "inputTokensAbove"],
        )
        .map_err(D::Error::custom)?;
        Ok(Self {
            rates: wire.rates,
            input_tokens_above: wire.input_tokens_above,
            properties,
        })
    }
}

impl Serialize for ModelCost {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(
            &ModelCostWire {
                rates: self.rates.clone(),
                tiers: self.tiers.clone(),
            },
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ModelCost {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        if value.get("tiers") == Some(&JsValue::Null) {
            return Err(D::Error::custom("model.cost.tiers cannot be null"));
        }
        let (wire, properties): (ModelCostWire, _) =
            ModelProperties::decode(value, &["input", "output", "cacheRead", "cacheWrite", "tiers"])
                .map_err(D::Error::custom)?;
        Ok(Self {
            rates: wire.rates,
            tiers: wire.tiers,
            properties,
        })
    }
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
    OpenAIResponses(Box<OpenAIResponsesCompat>),
    AnthropicMessages(Box<AnthropicMessagesCompat>),
    Other(JsValue),
}

impl CompatFields for ModelCompat {
    fn raw(&self, key: &str) -> Option<&JsValue> {
        match self {
            Self::OpenAICompletions(compat) => compat.raw(key),
            Self::OpenAIResponses(compat) => compat.raw(key),
            Self::AnthropicMessages(compat) => compat.raw(key),
            Self::Other(value) => value.get(key),
        }
    }
}

impl ModelCompat {
    /// Copy source properties without crossing JSON's UTF16/number output boundary.
    pub fn to_raw_value(&self) -> JsValue {
        match self {
            Self::OpenAICompletions(compat) => compat.to_raw_value(),
            Self::OpenAIResponses(compat) => compat.to_raw_value(),
            Self::AnthropicMessages(compat) => compat.to_raw_value(),
            Self::Other(value) => value.clone(),
        }
    }

    /// Known APIs expose raw compatibility records; custom APIs retain opaque
    /// JSON metadata instead of dropping the source's `compat` property.
    pub fn parse(api: &str, value: JsValue) -> Result<Self, serde_json::Error> {
        if value.as_object().is_none() {
            return Err(serde::de::Error::custom("model.compat must be an object"));
        }
        let compat = match api {
            "openai-completions" => Self::OpenAICompletions(Box::new(js_json::from_value(&value)?)),
            "openai-responses" | "azure-openai-responses" | "openai-codex-responses" => {
                Self::OpenAIResponses(Box::new(js_json::from_value(&value)?))
            }
            "anthropic-messages" => Self::AnthropicMessages(Box::new(js_json::from_value(&value)?)),
            _ => Self::Other(value),
        };
        Ok(compat)
    }

    pub fn validate_catalog(api: &str, value: &JsValue) -> Result<(), serde_json::Error> {
        match api {
            "openai-completions" => OpenAICompletionsCompat::validate_catalog(value),
            "openai-responses" | "azure-openai-responses" | "openai-codex-responses" => {
                OpenAIResponsesCompat::validate_catalog(value)
            }
            "anthropic-messages" => AnthropicMessagesCompat::validate_catalog(value),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: JsString,
    pub api: Api,
    pub provider: ProviderId,
    pub base_url: JsString,
    pub reasoning: bool,
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<Modality>,
    pub cost: ModelCost,
    pub context_window: f64,
    /// Exact context-token count at which automatic compaction begins.
    pub auto_compaction_threshold: Option<f64>,
    pub max_tokens: f64,
    pub sampling_params: Option<JsonObject>,
    pub headers: Option<IndexMap<String, JsString>>,
    pub compat: Option<ModelCompat>,
    /// Unknown properties and key order only; known values are stored once above.
    pub properties: ModelProperties,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelWire {
    id: String,
    name: JsString,
    api: Api,
    provider: ProviderId,
    base_url: JsString,
    reasoning: bool,
    thinking_level_map: Option<ThinkingLevelMap>,
    input: Vec<Modality>,
    cost: ModelCost,
    context_window: f64,
    auto_compaction_threshold: Option<f64>,
    max_tokens: f64,
    sampling_params: Option<JsonObject>,
    headers: Option<IndexMap<String, JsString>>,
    compat: Option<JsValue>,
}

const MODEL_FIELDS: &[&str] = &[
    "id",
    "name",
    "api",
    "provider",
    "baseUrl",
    "reasoning",
    "thinkingLevelMap",
    "input",
    "cost",
    "contextWindow",
    "autoCompactionThreshold",
    "maxTokens",
    "samplingParams",
    "headers",
    "compat",
];

fn validate_model_optionals(object: &JsObject) -> Result<(), String> {
    if object
        .get("id")
        .and_then(JsValue::as_string)
        .is_some_and(|id| id.to_utf8().is_err())
    {
        return Err("model.id contains an unpaired UTF-16 surrogate".into());
    }
    for field in [
        "thinkingLevelMap",
        "autoCompactionThreshold",
        "samplingParams",
        "headers",
        "compat",
    ] {
        if object.get(field) == Some(&JsValue::Null) {
            return Err(format!("model.{field} cannot be null"));
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSerialization<'a> {
    id: &'a str,
    name: &'a JsString,
    api: &'a str,
    provider: &'a str,
    base_url: &'a JsString,
    reasoning: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_level_map: Option<&'a ThinkingLevelMap>,
    input: &'a [Modality],
    cost: &'a ModelCost,
    #[serde(serialize_with = "js_number::serialize")]
    context_window: f64,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    auto_compaction_threshold: Option<f64>,
    #[serde(serialize_with = "js_number::serialize")]
    max_tokens: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    sampling_params: Option<&'a JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<&'a IndexMap<String, JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compat: Option<&'a ModelCompat>,
}

impl Serialize for Model {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let fields = ModelSerialization {
            id: &self.id,
            name: &self.name,
            api: &self.api,
            provider: &self.provider,
            base_url: &self.base_url,
            reasoning: self.reasoning,
            thinking_level_map: self.thinking_level_map.as_ref(),
            input: &self.input,
            cost: &self.cost,
            context_window: self.context_window,
            auto_compaction_threshold: self.auto_compaction_threshold,
            max_tokens: self.max_tokens,
            sampling_params: self.sampling_params.as_ref(),
            headers: self.headers.as_ref(),
            compat: self.compat.as_ref(),
        };
        self.properties.serialize(&fields, serializer)
    }
}

impl<'de> Deserialize<'de> for Model {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let JsValue::Object(object) = value else {
            return Err(D::Error::custom("model must be an object"));
        };
        validate_model_optionals(&object).map_err(D::Error::custom)?;
        let (known, properties) = ModelProperties::split(object, MODEL_FIELDS);
        let wire: ModelWire = pi_js_compat::js_json::from_value(&JsValue::Object(known)).map_err(D::Error::custom)?;
        let compat = match wire.compat {
            Some(value) => Some(ModelCompat::parse(&wire.api, value).map_err(D::Error::custom)?),
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
            properties,
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
    pub base_url: JsString,
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
    pub headers: Option<IndexMap<String, JsString>>,
}

fn deserialize_present_nullable<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}
