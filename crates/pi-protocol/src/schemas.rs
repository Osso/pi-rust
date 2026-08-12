use std::error::Error;
use std::fmt::{self, Display, Formatter};

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
