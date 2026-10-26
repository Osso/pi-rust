//! Tool capability name lists (`core/tool-capabilities.ts`).

pub const CHILD_DISABLED_AGENT_TOOL_NAMES: [&str; 9] = [
    "agent_viewer",
    "answer_question",
    "attach_session_agent",
    "close_agent",
    "list_agents",
    "set_agent_model",
    "spawn_agent",
    "steer_agent",
    "wait_agent",
];

pub const SUPERVISOR_ONLY_TOOL_NAMES: [&str; 2] = ["ask_supervisor", "manage_goal"];

pub static STANDALONE_DISABLED_TOOL_NAMES: std::sync::LazyLock<indexmap::IndexSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        CHILD_DISABLED_AGENT_TOOL_NAMES
            .into_iter()
            .chain(SUPERVISOR_ONLY_TOOL_NAMES)
            .chain([
                "send_agent_message",
                "contact_parent",
                "list_sessions",
                "channel_post",
                "resume_session",
            ])
            .collect()
    });
