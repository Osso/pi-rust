//! Bundled `run-plan` extension (`extensions/run-plan/src/index.ts`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_js_compat::js_json::{JsString, JsValue as Value, encode_node_utf8};
use pi_js_compat::js_regex::JsRegex;
use pi_js_compat::process_env;

use crate::core::extensions::types::{
    AgentEndEvent, BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType,
    SendUserMessageOptions, SessionStartEvent, UserMessageContent, UserMessageDelivery,
};
use crate::core::session_manager::KnownSessionEntry;
use crate::core::tools::node_fs::node_fs_error;

const ACTIVE_PLAN_ENTRY_TYPE: &str = "run-plan:active";
const RUN_PLAN_AGENT_INSTRUCTION: &str = "[run-plan: Do not read PLAN.md. Work on this selected item, then check off that exact item in PLAN.md when it is resolved.]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivePlan {
    file: JsString,
    path: JsString,
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
        Some("Submit the first unchecked item from PLAN.md or another markdown plan file.".into()),
        Some(Arc::new(|argument_prefix: &JsString| {
            let cwd = crate::utils::paths::process_cwd().map_err(|error| error.message().clone())?;
            let files = complete_markdown_files(&cwd, argument_prefix).map_err(|error| error.message)?;
            Ok(Some(
                files
                    .into_iter()
                    .map(|file| pi_js_compat::js_json!({"label": file, "value": file}))
                    .collect(),
            ))
        })),
        Arc::new(move |args: JsString, ctx: ExtensionCommandContext| {
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
pub fn find_next_plan_item(file_path: &JsString) -> Result<Option<JsString>, HandlerError> {
    let content = std::fs::read(encode_node_utf8(file_path))
        .map(|bytes| JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
        .map_err(|error| HandlerError::new(node_fs_error(&error, "open", file_path)))?;
    let pattern = unchecked_item_pattern();
    for line in content.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)) {
        if let Some(captures) = pattern.captures(&line) {
            return Ok(Some(captures.get(1).expect("item capture").text().trim()));
        }
    }
    Ok(None)
}

fn unchecked_item_pattern() -> &'static JsRegex {
    static PATTERN: std::sync::OnceLock<JsRegex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| JsRegex::new(r"^\s*[-*]\s+\[\s\]\s+(.*)$").expect("unchecked item pattern"))
}

fn read_persisted_active_plan(ctx: &ExtensionContext) -> Result<Option<ActivePlan>, HandlerError> {
    let entries = ctx.session_manager()?.get_entries();
    for entry in entries.iter().rev() {
        let known = entry.known();
        let Some(KnownSessionEntry::Custom(custom)) = known.as_deref() else {
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
        Some(plan) => pi_js_compat::js_json!({"file": basename(&plan.file), "path": plan.path}),
        None => Value::Null,
    };
    pi.append_entry(&ACTIVE_PLAN_ENTRY_TYPE.into(), Some(data))
}

fn resolve_plan_file(cwd: &JsString, args: &JsString) -> ActivePlan {
    let trimmed = args.trim();
    let file = if trimmed.is_empty() {
        JsString::from("PLAN.md")
    } else {
        trimmed
    };
    ActivePlan {
        file: file.to_owned(),
        path: join_plan_path(cwd, &file),
    }
}

fn build_plan_prompt(item: &JsString) -> JsString {
    pi_js_compat::js_format!("{}\n\n{}", item, RUN_PLAN_AGENT_INSTRUCTION)
}

fn plan_file_env_value(plan: &ActivePlan) -> JsString {
    let base = basename(&plan.file);
    if base == "PLAN.md" { "1".into() } else { base }
}

fn clear_plan_environment() {
    for name in ["PLAN_FILE", "PLAN_PATH", "PI_PLAN_FILE", "PI_PLAN_PATH"] {
        process_env::remove(name);
    }
}

fn export_plan_environment(plan: &ActivePlan) {
    process_env::set("PLAN_FILE", &encode_node_utf8(&plan_file_env_value(plan)));
    process_env::set("PLAN_PATH", &encode_node_utf8(&plan.path));
    process_env::set("PI_PLAN_FILE", &encode_node_utf8(&basename(&plan.file)));
    process_env::set("PI_PLAN_PATH", &encode_node_utf8(&plan.path));
}

async fn submit_next_plan_item(
    plan: &ActivePlan,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    follow_up: bool,
) -> Result<bool, HandlerError> {
    if !Path::new(&encode_node_utf8(&plan.path)).exists() {
        return Err(HandlerError::new(JsString::from("Plan file not found: ") + &plan.file));
    }

    let Some(next_item) = find_next_plan_item(&plan.path)? else {
        ctx.ui()?.notify(
            &(JsString::from("No unchecked items in ") + &plan.file),
            Some(NotifyType::Info),
        )?;
        return Ok(false);
    };
    if next_item.is_empty() {
        ctx.ui()?.notify(
            &(JsString::from("No unchecked items in ") + &plan.file),
            Some(NotifyType::Info),
        )?;
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
    args: &JsString,
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
    ctx.ui()?.set_editor_text(&"".into())?;
    Ok(Some(plan))
}

fn complete_markdown_files(cwd: &JsString, argument_prefix: &JsString) -> Result<Vec<JsString>, HandlerError> {
    let normalized_prefix = argument_prefix.trim();
    let entries = std::fs::read_dir(encode_node_utf8(cwd))
        .map_err(|error| HandlerError::new(node_fs_error(&error, "scandir", cwd)))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| HandlerError::new(node_fs_error(&error, "scandir", cwd)))?;
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        if name.ends_with(".md") && name.starts_with(&normalized_prefix) {
            files.push(name);
        }
    }
    files.sort_by(|left, right| pi_js_compat::collator::locale_compare_utf16(left.units(), right.units()));
    Ok(files)
}

fn basename(path: &JsString) -> JsString {
    path.split('/')
        .filter(|part| !part.is_empty())
        .last()
        .filter(|part| part != "." && part != "..")
        .unwrap_or_default()
}

fn join_plan_path(cwd: &JsString, file: &JsString) -> JsString {
    crate::utils::paths::node_join(cwd, file)
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn raw_plan_directory_survives_read_and_missing_file_error() {
        let temporary = tempfile::tempdir().unwrap();
        let cwd = JsString::from(temporary.path().to_str().expect("utf-8 test path"))
            + "/"
            + JsString::from_utf16(vec![0xd800]);
        std::fs::create_dir_all(encode_node_utf8(&cwd)).unwrap();
        let plan = resolve_plan_file(&cwd, &"PLAN.md".into());
        std::fs::write(encode_node_utf8(&plan.path), "- [ ] 🛠 native item\n").unwrap();
        assert_eq!(find_next_plan_item(&plan.path).unwrap(), Some("🛠 native item".into()));
        let missing = cwd + "/missing.md";
        // Node fs errors report the UTF-8-repaired path (pi_js_compat::node_fs format_fs_error_record).
        let error = find_next_plan_item(&missing).unwrap_err();
        assert!(error.message.contains(JsString::from(encode_node_utf8(&missing))));
    }

    #[test]
    fn plan_metadata_keeps_raw_filename_before_filesystem_encoding() {
        let name = JsString::from("plan-") + JsString::from_utf16(vec![0xd800]) + ".md";
        let plan = resolve_plan_file(&"/repo".into(), &name);
        assert_eq!(plan.file, name);
        assert_eq!(plan.path, JsString::from("/repo/") + &name);
        assert_eq!(basename(&plan.path), name);
    }
}
