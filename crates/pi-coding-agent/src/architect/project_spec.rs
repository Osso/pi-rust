//! Port of `packages/coding-agent/src/architect/project-spec.ts`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::json;

use crate::core::extensions::types::{ExtensionApi, HandlerError, ToolDefinition};

const SPEC_INDEX_PATH: [&str; 3] = ["docs", "specs", "README.md"];

/// `readProjectSpec`: reads a Markdown spec under `<projectRoot>/docs/specs`.
pub async fn read_project_spec(project_cwd: &str, spec_path: &str) -> Result<String, HandlerError> {
    let project_root = find_project_root(project_cwd).await?;
    let specs_root =
        std::fs::canonicalize(project_root.join(Path::new("docs").join("specs"))).map_err(project_io_error)?;
    let specs_relative_path = relative_path(&project_root, &specs_root);
    if specs_relative_path.starts_with("..") || specs_relative_path.is_absolute() {
        return Err(HandlerError::new(
            "Architect project specs directory must remain inside the project root",
        ));
    }
    let requested_path = resolve_path(&specs_root, spec_path);
    let lexical_relative_path = relative_path(&specs_root, &requested_path);
    if Path::new(spec_path).is_absolute()
        || lexical_relative_path.starts_with("..")
        || lexical_relative_path.is_absolute()
        || !requested_path.to_string_lossy().ends_with(".md")
    {
        return Err(HandlerError::new(
            "Architect project spec path must reference a relative Markdown file inside docs/specs",
        ));
    }
    let canonical_path = std::fs::canonicalize(&requested_path).map_err(project_io_error)?;
    let relative_path = relative_path(&specs_root, &canonical_path);
    if relative_path.starts_with("..") || relative_path.is_absolute() {
        return Err(HandlerError::new(
            "Architect project spec path must remain inside docs/specs",
        ));
    }
    // readFile(canonicalPath, "utf8")
    let bytes = tokio::fs::read(&canonical_path).await.map_err(project_io_error)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn project_io_error(error: std::io::Error) -> HandlerError {
    pi_ai::request::CallbackError::from_error(pi_js_compat::js_json::JsString::from(error.to_string()), error).into()
}

async fn find_project_root(start_cwd: &str) -> Result<PathBuf, HandlerError> {
    let mut candidate = if Path::new(start_cwd).is_absolute() {
        lexical_normalize(Path::new(start_cwd))
    } else {
        resolve_path(&process_cwd()?, start_cwd)
    };
    loop {
        let index = candidate.join(SPEC_INDEX_PATH.iter().collect::<PathBuf>());
        // readFile(index, "utf8"): only readability matters for root discovery.
        if tokio::fs::read(&index).await.is_ok() {
            return Ok(candidate);
        }
        let parent = candidate
            .parent()
            .filter(|parent| *parent != candidate)
            .ok_or_else(|| {
                HandlerError::new(format!(
                    "No docs/specs/README.md found above Architect project cwd: {start_cwd}"
                ))
            })?;
        candidate = parent.to_path_buf();
    }
}

fn process_cwd() -> Result<PathBuf, HandlerError> {
    pi_js_compat::process_cwd::cwd()
        .map(|cwd| PathBuf::from(pi_js_compat::js_json::encode_node_utf8(&cwd)))
        .map_err(HandlerError::from)
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
        label: "Read project spec".into(),
        description: "Read an authoritative Markdown spec from an explicit Architect request's project.".into(),
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
    let content = read_project_spec(&project_cwd.to_utf8_lossy(), &spec_path.to_utf8_lossy()).await?;
    Ok(AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(content))],
        details: Some(pi_js_compat::js_json!({"path": spec_path})),
        is_error: None,
        terminate: None,
    })
}

/// `registerArchitectProjectSpecTool`.
pub fn register_architect_project_spec_tool(
    pi: &ExtensionApi,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    pi.register_tool(architect_project_spec_tool_definition())
}
