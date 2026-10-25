//! Port of `packages/coding-agent/extensions/goal/src/*.ts`.

pub mod completion_scheduling;
pub mod empty_response_scheduling;
pub mod error_status_scheduling;
pub mod goal_agent_end;
pub mod goal_agent_end_scheduling;
pub mod goal_args;
pub mod goal_idle_selection;
pub mod goal_objective;
pub mod goal_parsing;
pub mod goal_presentation;
pub mod goal_review_evidence;
pub mod goal_scheduling;
pub mod goal_state;
pub mod goal_tool;
pub mod goal_turn;
pub mod goal_types;
pub mod index;
pub mod rendering;
pub mod stale_context;
pub mod supervisor_review;
pub mod wait_countdown;
pub mod wait_status;

pub use index::{goal_extension_factory, install_goal_extension};

use crate::core::extensions::ExtensionRegistry;

/// Registers `<first-party:goal>` with default options.
pub fn register(registry: &mut ExtensionRegistry) {
    registry.register(
        "<first-party:goal>",
        goal_extension_factory(goal_types::GoalExtensionOptions::default()),
    );
}
