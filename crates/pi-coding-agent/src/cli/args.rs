use pi_agent_core::harness::ThinkingLevel;

const VALID_THINKING_LEVELS: [ThinkingLevel; 8] = [
    ThinkingLevel::Off,
    ThinkingLevel::Minimal,
    ThinkingLevel::Low,
    ThinkingLevel::Medium,
    ThinkingLevel::High,
    ThinkingLevel::XHigh,
    ThinkingLevel::Max,
    ThinkingLevel::Ultra,
];

pub fn is_valid_thinking_level(level: &str) -> bool {
    VALID_THINKING_LEVELS
        .iter()
        .any(|valid_level| valid_level.as_str() == level)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Text,
    Json,
    Rpc,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Json => "json",
            Self::Rpc => "rpc",
        }
    }
}
