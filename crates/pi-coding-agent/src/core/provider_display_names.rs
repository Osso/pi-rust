//! Built-in provider display names (`core/provider-display-names.ts`).

/// Provider id → display name, in source declaration order.
pub const BUILT_IN_PROVIDER_DISPLAY_NAMES: [(&str, &str); 32] = [
    ("anthropic", "Anthropic"),
    ("amazon-bedrock", "Amazon Bedrock"),
    ("ant-ling", "Ant Ling"),
    ("cerebras", "Cerebras"),
    ("cloudflare-ai-gateway", "Cloudflare AI Gateway"),
    ("cloudflare-workers-ai", "Cloudflare Workers AI"),
    ("deepseek", "DeepSeek"),
    ("fireworks", "Fireworks"),
    ("google", "Google Gemini"),
    ("google-vertex", "Google Vertex AI"),
    ("groq", "Groq"),
    ("huggingface", "Hugging Face"),
    ("kimi-coding", "Kimi For Coding"),
    ("mistral", "Mistral"),
    ("minimax", "MiniMax"),
    ("minimax-cn", "MiniMax (China)"),
    ("moonshotai", "Moonshot AI"),
    ("moonshotai-cn", "Moonshot AI (China)"),
    ("nvidia", "NVIDIA NIM"),
    ("opencode", "OpenCode Zen"),
    ("opencode-go", "OpenCode Go"),
    ("openai", "OpenAI"),
    ("openrouter", "OpenRouter"),
    ("together", "Together AI"),
    ("vercel-ai-gateway", "Vercel AI Gateway"),
    ("xai", "xAI"),
    ("zai", "ZAI Coding Plan (Global)"),
    ("zai-coding-cn", "ZAI Coding Plan (China)"),
    ("xiaomi", "Xiaomi MiMo"),
    ("xiaomi-token-plan-cn", "Xiaomi MiMo Token Plan (China)"),
    ("xiaomi-token-plan-ams", "Xiaomi MiMo Token Plan (Amsterdam)"),
    ("xiaomi-token-plan-sgp", "Xiaomi MiMo Token Plan (Singapore)"),
];

/// Record lookup `BUILT_IN_PROVIDER_DISPLAY_NAMES[providerId]`.
pub fn built_in_provider_display_name(provider_id: &str) -> Option<&'static str> {
    BUILT_IN_PROVIDER_DISPLAY_NAMES
        .iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, name)| *name)
}
