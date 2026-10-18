//! Port of `packages/coding-agent/src/core/export-html/index.ts`.

pub mod ansi_to_html;
pub mod index;
pub mod tool_renderer;

pub use ansi_to_html::{ansi_lines_to_html, ansi_to_html};
pub use index::{
    ExportError, ExportOptions, RenderedToolHtml, RenderedToolResult, SessionData, ToolHtmlRenderer, ToolSummary,
    export_from_file, export_session_to_html,
};
pub use tool_renderer::{
    ExtensionToolHtmlRenderer, ToolDefinitionLookup, ToolHtmlRendererDeps, create_tool_html_renderer,
};
