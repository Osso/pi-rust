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
