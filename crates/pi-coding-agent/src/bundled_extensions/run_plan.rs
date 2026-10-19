//! Bundled `run-plan` extension (`extensions/run-plan/src/index.ts`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_js_compat::process_env;
use regex::Regex;
use serde_json::{Value, json};

use crate::core::extensions::types::{
    AgentEndEvent, BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType,
    SendUserMessageOptions, SessionStartEvent, UserMessageContent, UserMessageDelivery,
};
use crate::core::session_manager::{KnownSessionEntry, SessionEntry};
use crate::core::tools::node_fs::node_fs_error;
use crate::utils::paths::node_join;

const ACTIVE_PLAN_ENTRY_TYPE: &str = "run-plan:active";
const RUN_PLAN_AGENT_INSTRUCTION: &str = "[run-plan: Do not read PLAN.md. Work on this selected item, then check off that exact item in PLAN.md when it is resolved.]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePlan {
    file: String,
    path: String,
}

pub fn run_plan_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let active_plan: Arc<Mutex<Option<ActivePlan>>> = Arc::new(Mutex::new(None));

    let session_plan = Arc::clone(&active_plan);
    pi.on_session_start(
        move |_event: SessionStartEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let session_plan = Arc::clone(&session_plan);
            Box::pin(async move {
                clear_plan_environment();
                let persisted = read_persisted_active_plan(&ctx)?;
                if let Some(plan) = &persisted {
                    export_plan_environment(plan);
                }
                *session_plan.lock().unwrap() = persisted;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;

    let end_plan = Arc::clone(&active_plan);
    let end_api = pi.clone();
    pi.on_agent_end(
        move |event: AgentEndEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let end_plan = Arc::clone(&end_plan);
            let end_api = end_api.clone();
            Box::pin(async move {
                if event.session_continuation.is_some() {
                    return Ok(None);
                }
                let plan = end_plan.lock().unwrap().clone();
                let Some(plan) = plan else {
                    return Ok(None);
                };
                if !submit_next_plan_item(&plan, &ctx, &end_api, true).await? {
                    *end_plan.lock().unwrap() = None;
                    persist_active_plan(&end_api, None)?;
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;

    let command_plan = Arc::clone(&active_plan);
    let command_api = pi.clone();
    pi.register_command(
        "run-plan",
        Some("Submit the first unchecked item from PLAN.md or another markdown plan file.".to_owned()),
        Some(Arc::new(|argument_prefix: &str| {
            let cwd = std::env::current_dir().ok()?;
            complete_markdown_files(&cwd.to_string_lossy(), argument_prefix)
                .ok()
                .map(|files| {
                    files
                        .into_iter()
                        .map(|file| json!({"label": file, "value": file}))
                        .collect()
                })
        })),
        Arc::new(move |args: String, ctx: ExtensionCommandContext| {
            let command_plan = Arc::clone(&command_plan);
            let command_api = command_api.clone();
            Box::pin(async move {
                let plan = run_plan(&args, &ctx, &command_api).await?;
                *command_plan.lock().unwrap() = plan;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

/// `findNextPlanItem`.
pub fn find_next_plan_item(file_path: &str) -> Result<Option<String>, HandlerError> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|error| HandlerError::new(node_fs_error(&error, "open", file_path)))?;
    let pattern = unchecked_item_pattern();
    for line in content.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)) {
        if let Some(captures) = pattern.captures(line) {
            return Ok(Some(captures[1].trim().to_owned()));
        }
    }
    Ok(None)
}

fn unchecked_item_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^\s*[-*]\s+\[\s\]\s+(.*)$").expect("unchecked item pattern"))
}

fn read_persisted_active_plan(ctx: &ExtensionContext) -> Result<Option<ActivePlan>, HandlerError> {
    let entries = ctx.session_manager()?.get_entries();
    for entry in entries.iter().rev() {
        let SessionEntry::Known(KnownSessionEntry::Custom(custom)) = entry else {
            continue;
        };
        if custom.custom_type != ACTIVE_PLAN_ENTRY_TYPE {
            continue;
        }
        let Some(Value::Object(record)) = &custom.data else {
            return Ok(None);
        };
        let (Some(Value::String(file)), Some(Value::String(path))) = (record.get("file"), record.get("path")) else {
            return Ok(None);
        };
        return Ok(Some(ActivePlan {
            file: file.clone(),
            path: path.clone(),
        }));
    }
    Ok(None)
}

fn persist_active_plan(pi: &ExtensionApi, plan: Option<&ActivePlan>) -> Result<(), HandlerError> {
    let data = match plan {
        Some(plan) => json!({"file": basename(&plan.file), "path": plan.path}),
        None => Value::Null,
    };
    pi.append_entry(ACTIVE_PLAN_ENTRY_TYPE, Some(data))
}

fn resolve_plan_file(cwd: &str, args: &str) -> ActivePlan {
    let trimmed = args.trim();
    let file = if trimmed.is_empty() { "PLAN.md" } else { trimmed };
    ActivePlan {
        file: file.to_owned(),
        path: node_join(cwd, file),
    }
}

fn build_plan_prompt(item: &str) -> String {
    format!("{item}\n\n{RUN_PLAN_AGENT_INSTRUCTION}")
}

fn plan_file_env_value(plan: &ActivePlan) -> String {
    let base = basename(&plan.file);
    if base == "PLAN.md" { "1".to_owned() } else { base }
}

fn clear_plan_environment() {
    for name in ["PLAN_FILE", "PLAN_PATH", "PI_PLAN_FILE", "PI_PLAN_PATH"] {
        process_env::remove(name);
    }
}

fn export_plan_environment(plan: &ActivePlan) {
    process_env::set("PLAN_FILE", &plan_file_env_value(plan));
    process_env::set("PLAN_PATH", &plan.path);
    process_env::set("PI_PLAN_FILE", &basename(&plan.file));
    process_env::set("PI_PLAN_PATH", &plan.path);
}

async fn submit_next_plan_item(
    plan: &ActivePlan,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    follow_up: bool,
) -> Result<bool, HandlerError> {
    if !Path::new(&plan.path).exists() {
        return Err(HandlerError::new(format!("Plan file not found: {}", plan.file)));
    }

    let Some(next_item) = find_next_plan_item(&plan.path)? else {
        ctx.ui()?
            .notify(&format!("No unchecked items in {}", plan.file), Some(NotifyType::Info));
        return Ok(false);
    };
    if next_item.is_empty() {
        ctx.ui()?
            .notify(&format!("No unchecked items in {}", plan.file), Some(NotifyType::Info));
        return Ok(false);
    }

    let prompt = build_plan_prompt(&next_item);
    let options = follow_up.then_some(SendUserMessageOptions {
        deliver_as: Some(UserMessageDelivery::FollowUp),
        expand_prompt_templates: None,
    });
    pi.send_user_message(UserMessageContent::Text(prompt), options)?;
    Ok(true)
}

async fn run_plan(
    args: &str,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
) -> Result<Option<ActivePlan>, HandlerError> {
    if !ctx.is_idle()? {
        return Err(HandlerError::new("/run-plan is blocked while a task is running"));
    }

    let plan = resolve_plan_file(&ctx.cwd()?, args);
    if !submit_next_plan_item(&plan, ctx, pi, false).await? {
        return Ok(None);
    }

    export_plan_environment(&plan);
    persist_active_plan(pi, Some(&plan))?;
    ctx.ui()?.set_editor_text("");
    Ok(Some(plan))
}

fn complete_markdown_files(cwd: &str, argument_prefix: &str) -> Result<Vec<String>, HandlerError> {
    let normalized_prefix = argument_prefix.trim();
    let mut files: Vec<String> = std::fs::read_dir(cwd)
        .map_err(|error| HandlerError::new(node_fs_error(&error, "scandir", cwd)))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|entry| entry.ends_with(".md"))
        .filter(|entry| entry.starts_with(normalized_prefix))
        .collect();
    files.sort_by(|left, right| pi_js_compat::locale_compare(left, right));
    Ok(files)
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}
