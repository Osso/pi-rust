//! Tool capability name lists (`core/tool-capabilities.ts`).

pub const CHILD_DISABLED_AGENT_TOOL_NAMES: [&str; 7] = [
    "agent_viewer",
    "attach_session_agent",
    "close_agent",
    "list_agents",
    "spawn_agent",
    "steer_agent",
    "wait_agent",
];

pub const SUPERVISOR_ONLY_TOOL_NAMES: [&str; 2] = ["ask_supervisor", "manage_goal"];
