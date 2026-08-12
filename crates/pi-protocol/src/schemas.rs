use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Thinking levels exposed by the protocol schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl ThinkingLevel {
    /// Returns the exact protocol literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

/// Returned when a string is not a protocol thinking-level literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseThinkingLevelError;

impl Display for ParseThinkingLevelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown protocol thinking level")
    }
}

impl Error for ParseThinkingLevelError {}

impl FromStr for ThinkingLevel {
    type Err = ParseThinkingLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::XHigh),
            "max" => Ok(Self::Max),
            _ => Err(ParseThinkingLevelError),
        }
    }
}

/// Session phases exposed by the protocol schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    Idle,
    Turn,
    Compaction,
    BranchSummary,
    Retry,
}

impl SessionPhase {
    /// Returns the exact protocol literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Turn => "turn",
            Self::Compaction => "compaction",
            Self::BranchSummary => "branch_summary",
            Self::Retry => "retry",
        }
    }
}

/// Returned when a string is not a protocol session-phase literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSessionPhaseError;

impl Display for ParseSessionPhaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown protocol session phase")
    }
}

impl Error for ParseSessionPhaseError {}

impl FromStr for SessionPhase {
    type Err = ParseSessionPhaseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "idle" => Ok(Self::Idle),
            "turn" => Ok(Self::Turn),
            "compaction" => Ok(Self::Compaction),
            "branch_summary" => Ok(Self::BranchSummary),
            "retry" => Ok(Self::Retry),
            _ => Err(ParseSessionPhaseError),
        }
    }
}

/// Provider and model identifiers used by protocol messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    provider: String,
    id: String,
}

impl ModelRef {
    /// Creates a model reference whose fields satisfy the source `minLength: 1` constraints.
    pub fn new(provider: String, id: String) -> Result<Self, ModelRefError> {
        if provider.is_empty() {
            return Err(ModelRefError::EmptyProvider);
        }
        if id.is_empty() {
            return Err(ModelRefError::EmptyId);
        }
        Ok(Self { provider, id })
    }

    /// Provider identifier.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Provider-specific model identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Identifies the first empty model-reference field in source schema order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRefError {
    EmptyProvider,
    EmptyId,
}

impl Display for ModelRefError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::EmptyProvider => "provider",
            Self::EmptyId => "id",
        };
        write!(formatter, "Model reference field {field} must not be empty")
    }
}

impl Error for ModelRefError {}

/// Per-million-token model prices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelCost {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
}

impl ModelCost {
    /// Creates model costs after applying the source schema's finite and non-negative constraints.
    pub fn new(input: f64, output: f64, cache_read: f64, cache_write: f64) -> Result<Self, ModelCostError> {
        Ok(Self {
            input: validate_cost(input, ModelCostError::InvalidInput)?,
            output: validate_cost(output, ModelCostError::InvalidOutput)?,
            cache_read: validate_cost(cache_read, ModelCostError::InvalidCacheRead)?,
            cache_write: validate_cost(cache_write, ModelCostError::InvalidCacheWrite)?,
        })
    }

    /// Input-token price.
    pub const fn input(self) -> f64 {
        self.input
    }

    /// Output-token price.
    pub const fn output(self) -> f64 {
        self.output
    }

    /// Cached-input read price.
    pub const fn cache_read(self) -> f64 {
        self.cache_read
    }

    /// Cached-input write price.
    pub const fn cache_write(self) -> f64 {
        self.cache_write
    }
}

/// Identifies the first model-cost field that violates its source schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCostError {
    InvalidInput,
    InvalidOutput,
    InvalidCacheRead,
    InvalidCacheWrite,
}

impl Display for ModelCostError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::InvalidInput => "input",
            Self::InvalidOutput => "output",
            Self::InvalidCacheRead => "cacheRead",
            Self::InvalidCacheWrite => "cacheWrite",
        };
        write!(
            formatter,
            "Model cost field {field} must be a finite number greater than or equal to 0"
        )
    }
}

impl Error for ModelCostError {}

fn validate_cost(value: f64, error: ModelCostError) -> Result<f64, ModelCostError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(error)
    }
}

/// Input modalities exposed by model metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelInput {
    Text,
    Image,
}

impl ModelInput {
    /// Returns the exact protocol literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
        }
    }
}

/// Returned when a string is not a protocol model-input literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseModelInputError;

impl Display for ParseModelInputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown protocol model input")
    }
}

impl Error for ParseModelInputError {}

impl FromStr for ModelInput {
    type Err = ParseModelInputError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            _ => Err(ParseModelInputError),
        }
    }
}

/// Values used to construct validated model metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetadataInput {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub api: String,
    pub reasoning: bool,
    pub input: Vec<ModelInput>,
    pub context_window: u64,
    pub max_tokens: u64,
    pub cost: ModelCost,
    pub supported_thinking_levels: Vec<ThinkingLevel>,
    pub authenticated: bool,
}

/// Model information published through the protocol.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetadata {
    provider: String,
    id: String,
    name: String,
    api: String,
    reasoning: bool,
    input: Vec<ModelInput>,
    context_window: u64,
    max_tokens: u64,
    cost: ModelCost,
    supported_thinking_levels: Vec<ThinkingLevel>,
    authenticated: bool,
}

impl ModelMetadata {
    /// Creates model metadata after applying the source schema's typed constraints.
    pub fn new(input: ModelMetadataInput) -> Result<Self, ModelMetadataError> {
        validate_metadata_text(&input.provider, ModelMetadataError::EmptyProvider)?;
        validate_metadata_text(&input.id, ModelMetadataError::EmptyId)?;
        validate_metadata_text(&input.name, ModelMetadataError::EmptyName)?;
        validate_metadata_text(&input.api, ModelMetadataError::EmptyApi)?;
        validate_metadata_count(input.context_window, ModelMetadataError::ZeroContextWindow)?;
        validate_metadata_count(input.max_tokens, ModelMetadataError::ZeroMaxTokens)?;
        if input.supported_thinking_levels.is_empty() {
            return Err(ModelMetadataError::EmptySupportedThinkingLevels);
        }

        Ok(Self {
            provider: input.provider,
            id: input.id,
            name: input.name,
            api: input.api,
            reasoning: input.reasoning,
            input: input.input,
            context_window: input.context_window,
            max_tokens: input.max_tokens,
            cost: input.cost,
            supported_thinking_levels: input.supported_thinking_levels,
            authenticated: input.authenticated,
        })
    }

    /// Provider identifier.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Provider-specific model identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable model name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Provider API identifier.
    pub fn api(&self) -> &str {
        &self.api
    }

    /// Whether the model supports reasoning.
    pub const fn reasoning(&self) -> bool {
        self.reasoning
    }

    /// Supported input modalities in source order.
    pub fn input(&self) -> &[ModelInput] {
        &self.input
    }

    /// Model context-window size.
    pub const fn context_window(&self) -> u64 {
        self.context_window
    }

    /// Maximum output-token count.
    pub const fn max_tokens(&self) -> u64 {
        self.max_tokens
    }

    /// Per-million-token model prices.
    pub const fn cost(&self) -> ModelCost {
        self.cost
    }

    /// Supported thinking levels in source order.
    pub fn supported_thinking_levels(&self) -> &[ThinkingLevel] {
        &self.supported_thinking_levels
    }

    /// Whether credentials are available for the model.
    pub const fn authenticated(&self) -> bool {
        self.authenticated
    }
}

/// Identifies the first model-metadata field that violates the typed constructor contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelMetadataError {
    EmptyProvider,
    EmptyId,
    EmptyName,
    EmptyApi,
    ZeroContextWindow,
    ZeroMaxTokens,
    EmptySupportedThinkingLevels,
}

impl Display for ModelMetadataError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProvider => formatter.write_str("Model metadata field provider must not be empty"),
            Self::EmptyId => formatter.write_str("Model metadata field id must not be empty"),
            Self::EmptyName => formatter.write_str("Model metadata field name must not be empty"),
            Self::EmptyApi => formatter.write_str("Model metadata field api must not be empty"),
            Self::ZeroContextWindow => formatter.write_str("Model metadata field contextWindow must be greater than 0"),
            Self::ZeroMaxTokens => formatter.write_str("Model metadata field maxTokens must be greater than 0"),
            Self::EmptySupportedThinkingLevels => {
                formatter.write_str("Model metadata field supportedThinkingLevels must contain at least one item")
            }
        }
    }
}

impl Error for ModelMetadataError {}

fn validate_metadata_text(value: &str, error: ModelMetadataError) -> Result<(), ModelMetadataError> {
    if value.is_empty() {
        return Err(error);
    }
    Ok(())
}

fn validate_metadata_count(value: u64, error: ModelMetadataError) -> Result<(), ModelMetadataError> {
    if value == 0 {
        return Err(error);
    }
    Ok(())
}
