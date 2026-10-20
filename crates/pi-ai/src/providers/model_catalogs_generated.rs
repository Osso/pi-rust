/// Catalog modules in `models.generated.ts` order.
const PROVIDER_CATALOGS: &[(&str, &str)] = &[
    (
        "amazon-bedrock",
        include_str!("../../data/providers/amazon-bedrock.json"),
    ),
    ("ant-ling", include_str!("../../data/providers/ant-ling.json")),
    ("anthropic", include_str!("../../data/providers/anthropic.json")),
    (
        "azure-openai-responses",
        include_str!("../../data/providers/azure-openai-responses.json"),
    ),
    ("baseten", include_str!("../../data/providers/baseten.json")),
    ("cerebras", include_str!("../../data/providers/cerebras.json")),
    (
        "cloudflare-ai-gateway",
        include_str!("../../data/providers/cloudflare-ai-gateway.json"),
    ),
    (
        "cloudflare-workers-ai",
        include_str!("../../data/providers/cloudflare-workers-ai.json"),
    ),
    ("deepseek", include_str!("../../data/providers/deepseek.json")),
    ("fireworks", include_str!("../../data/providers/fireworks.json")),
    (
        "github-copilot",
        include_str!("../../data/providers/github-copilot.json"),
    ),
    ("google", include_str!("../../data/providers/google.json")),
    ("google-vertex", include_str!("../../data/providers/google-vertex.json")),
    ("groq", include_str!("../../data/providers/groq.json")),
    ("huggingface", include_str!("../../data/providers/huggingface.json")),
    ("kimi-coding", include_str!("../../data/providers/kimi-coding.json")),
    ("minimax", include_str!("../../data/providers/minimax.json")),
    ("minimax-cn", include_str!("../../data/providers/minimax-cn.json")),
    ("mistral", include_str!("../../data/providers/mistral.json")),
    ("moonshotai", include_str!("../../data/providers/moonshotai.json")),
    ("moonshotai-cn", include_str!("../../data/providers/moonshotai-cn.json")),
    ("nvidia", include_str!("../../data/providers/nvidia.json")),
    ("openai", include_str!("../../data/providers/openai.json")),
    ("openai-codex", include_str!("../../data/providers/openai-codex.json")),
    (
        "openai-codex-gc",
        include_str!("../../data/providers/openai-codex-gc.json"),
    ),
    ("opencode", include_str!("../../data/providers/opencode.json")),
    ("opencode-go", include_str!("../../data/providers/opencode-go.json")),
    ("openrouter", include_str!("../../data/providers/openrouter.json")),
    (
        "qwen-token-plan",
        include_str!("../../data/providers/qwen-token-plan.json"),
    ),
    (
        "qwen-token-plan-cn",
        include_str!("../../data/providers/qwen-token-plan-cn.json"),
    ),
    (
        "qwen-token-plan-individual",
        include_str!("../../data/providers/qwen-token-plan-individual.json"),
    ),
    ("together", include_str!("../../data/providers/together.json")),
    (
        "vercel-ai-gateway",
        include_str!("../../data/providers/vercel-ai-gateway.json"),
    ),
    ("xai", include_str!("../../data/providers/xai.json")),
    ("xiaomi", include_str!("../../data/providers/xiaomi.json")),
    (
        "xiaomi-token-plan-ams",
        include_str!("../../data/providers/xiaomi-token-plan-ams.json"),
    ),
    (
        "xiaomi-token-plan-cn",
        include_str!("../../data/providers/xiaomi-token-plan-cn.json"),
    ),
    (
        "xiaomi-token-plan-sgp",
        include_str!("../../data/providers/xiaomi-token-plan-sgp.json"),
    ),
    ("zai", include_str!("../../data/providers/zai.json")),
    ("zai-coding-cn", include_str!("../../data/providers/zai-coding-cn.json")),
];
