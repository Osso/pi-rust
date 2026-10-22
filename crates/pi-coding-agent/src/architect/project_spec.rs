//! Port of `packages/coding-agent/src/architect/project-spec.ts`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::json;

use crate::core::extensions::types::{ExtensionApi, HandlerError, ToolDefinition};

const SPEC_INDEX_PATH: [&str; 3] = ["docs", "specs", "README.md"];

/// `readProjectSpec`: reads a Markdown spec under `<projectRoot>/docs/specs`.
pub async fn read_project_spec(project_cwd: &str, spec_path: &str) -> Result<String, String> {
    let project_root = find_project_root(project_cwd).await?;
    let specs_root =
        std::fs::canonicalize(project_root.join(Path::new("docs").join("specs"))).map_err(|error| error.to_string())?;
    let specs_relative_path = relative_path(&project_root, &specs_root);
    if specs_relative_path.starts_with("..") || specs_relative_path.is_absolute() {
        return Err("Architect project specs directory must remain inside the project root".to_owned());
    }
    let requested_path = resolve_path(&specs_root, spec_path);
    let lexical_relative_path = relative_path(&specs_root, &requested_path);
    if Path::new(spec_path).is_absolute()
        || lexical_relative_path.starts_with("..")
        || lexical_relative_path.is_absolute()
        || !requested_path.to_string_lossy().ends_with(".md")
    {
        return Err("Architect project spec path must reference a relative Markdown file inside docs/specs".to_owned());
    }
    let canonical_path = std::fs::canonicalize(&requested_path).map_err(|error| error.to_string())?;
    let relative_path = relative_path(&specs_root, &canonical_path);
    if relative_path.starts_with("..") || relative_path.is_absolute() {
        return Err("Architect project spec path must remain inside docs/specs".to_owned());
    }
    tokio::fs::read_to_string(&canonical_path)
        .await
        .map_err(|error| error.to_string())
}

async fn find_project_root(start_cwd: &str) -> Result<PathBuf, String> {
    let mut candidate = resolve_path(&process_cwd(), start_cwd);
    loop {
        let index = candidate.join(SPEC_INDEX_PATH.iter().collect::<PathBuf>());
        if tokio::fs::read_to_string(&index).await.is_ok() {
            return Ok(candidate);
        }
        let Some(parent) = candidate.parent().map(Path::to_path_buf) else {
            return Err(format!(
                "No docs/specs/README.md found above Architect project cwd: {start_cwd}"
            ));
        };
        if parent == candidate {
            return Err(format!(
                "No docs/specs/README.md found above Architect project cwd: {start_cwd}"
            ));
        }
        candidate = parent;
    }
}

fn process_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return lexical_normalize(candidate);
    }
    lexical_normalize(&base.join(candidate))
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn relative_path(base: &Path, target: &Path) -> PathBuf {
    let base_components: Vec<_> = base.components().collect();
    let target_components: Vec<_> = target.components().collect();
    let shared = base_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut relative = PathBuf::new();
    for _ in shared..base_components.len() {
        relative.push("..");
    }
    for component in &target_components[shared..] {
        relative.push(component.as_os_str());
    }
    relative
}

/// `registerArchitectProjectSpecTool` tool definition.
pub fn architect_project_spec_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_project_spec".to_owned(),
        label: "Read project spec".to_owned(),
        description: "Read an authoritative Markdown spec from an explicit Architect request's project.".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: Some(Vec::new()),
        approval_required: Some(false),
        approval_kind: None,
        parameters: json!({
            "type": "object",
            "properties": {
                "project_cwd": {
                    "type": "string",
                    "description": "Exact projectCwd from the Architect request."
                },
                "spec_path": {
                    "type": "string",
                    "description": "Path relative to docs/specs, such as README.md."
                }
            },
            "required": ["project_cwd", "spec_path"]
        }),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_tool_call_id, params, _signal, _on_update, _context| {
            Box::pin(async move { execute_project_spec_tool(&params).await })
        }),
        render_call: None,
        render_result: None,
    }
}

async fn execute_project_spec_tool(params: &pi_js_compat::js_json::JsValue) -> Result<AgentToolResult, HandlerError> {
    let project_cwd = params
        .get("project_cwd")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| HandlerError::new("read_project_spec requires a string project_cwd"))?;
    let spec_path = params
        .get("spec_path")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .ok_or_else(|| HandlerError::new("read_project_spec requires a string spec_path"))?;
    let content = read_project_spec(&project_cwd.to_utf8_lossy(), &spec_path.to_utf8_lossy())
        .await
        .map_err(HandlerError::new)?;
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(content))],
        details: Some(pi_js_compat::js_json!({"path": spec_path})),
        is_error: None,
        terminate: None,
    })
}

/// `registerArchitectProjectSpecTool`.
pub fn register_architect_project_spec_tool(pi: &ExtensionApi) -> Result<(), String> {
    pi.register_tool(architect_project_spec_tool_definition())
}
