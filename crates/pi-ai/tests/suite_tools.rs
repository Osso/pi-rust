// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "deferred_tools.rs"]
pub mod deferred_tools;
#[path = "differential_deferred_tools.rs"]
pub mod differential_deferred_tools;
#[path = "generator_repin_filesystem.rs"]
pub mod generator_repin_filesystem;
#[path = "tool_description.rs"]
pub mod tool_description;
#[path = "tool_metadata.rs"]
pub mod tool_metadata;
