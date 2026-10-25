// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "support/oracle.rs"]
mod shared_oracle;

#[path = "generator_repin_filesystem.rs"]
pub mod generator_repin_filesystem;
#[path = "tool_description.rs"]
pub mod tool_description;
#[path = "tool_metadata.rs"]
pub mod tool_metadata;
