//! Frozen Codex Daybreak Blue command, session state, and request payload hook.

use std::sync::{Arc, Mutex};

use pi_ai::types::Model;
use pi_js_compat::js_json::{JsString, JsValue};

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, NotifyType};
use crate::core::footer_data_provider::ExtensionStatus;
use crate::core::session_manager::KnownSessionEntry;

const DAYBREAK_ENTRY: &str = "codex-daybreak";
const DAYBREAK_BLUE: &str = "daybreak_blue";
const BLUE_ELIGIBLE_MODELS: &[&str] = &["gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.5"];
type SharedState = Arc<Mutex<bool>>;

fn read_state(state: &SharedState) -> bool {
    *state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_state(state: &SharedState, enabled: bool) {
    *state.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = enabled;
}

fn is_child_runtime(ctx: &ExtensionContext) -> Result<bool, HandlerError> {
    Ok(ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()?)
}

fn applies_to(model: Option<&Model>) -> bool {
    super::codex_fast::supports_fast_mode(model)
        && model.is_some_and(|model| BLUE_ELIGIBLE_MODELS.contains(&model.id.as_str()))
}

fn update_status(ctx: &ExtensionContext, enabled: bool, model: Option<&Model>) -> Result<(), HandlerError> {
    let status = enabled.then(|| {
        let text = if applies_to(model) {
            "daybreak blue"
        } else {
            "daybreak blue inactive"
        };
        ExtensionStatus::Text(text.into())
    });
    ctx.ui()?.set_status(&DAYBREAK_ENTRY.into(), status)?;
    Ok(())
}

fn read_persisted_program(ctx: &ExtensionContext) -> Result<bool, HandlerError> {
    let manager = ctx.session_manager()?;
    let branch = manager.get_branch(None);
    for entry in branch.iter().rev() {
        let known = entry.known();
        let Some(KnownSessionEntry::Custom(custom)) = known.as_deref() else {
            continue;
        };
        if custom.custom_type != DAYBREAK_ENTRY {
            continue;
        }
        let Some(data) = custom.data.as_ref().and_then(JsValue::as_object) else {
            continue;
        };
        match data.get("cyber") {
            Some(JsValue::Null) => return Ok(false),
            Some(JsValue::String(program)) if program == DAYBREAK_BLUE => return Ok(true),
            _ => {}
        }
    }
    Ok(false)
}

fn notify_selection(ctx: &ExtensionContext, enabled: bool) -> Result<(), HandlerError> {
    let model = ctx.model()?;
    if enabled && !applies_to(model.as_ref()) {
        let id = model.as_ref().map_or("no model", |model| model.id.as_str());
        ctx.ui()?.notify(
            &format!("Daybreak: blue (not applied to {id})").into(),
            Some(NotifyType::Warning),
        )?;
        return Ok(());
    }
    let text = if enabled { "Daybreak: blue" } else { "Daybreak: off" };
    ctx.ui()?.notify(&text.into(), Some(NotifyType::Info))?;
    Ok(())
}

fn apply_command(
    args: &JsString,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    state: &SharedState,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if is_child_runtime(ctx)? {
        ui.notify(
            &"Daybreak applies only to the main thread".into(),
            Some(NotifyType::Warning),
        )?;
        return Ok(());
    }
    let requested = args.trim().to_lowercase();
    if requested.is_empty() {
        return notify_selection(ctx, read_state(state));
    }
    let enabled = match requested {
        value if value == "blue" => true,
        value if value == "off" => false,
        _ => {
            ui.notify(&"Usage: /daybreak [blue|off]".into(), Some(NotifyType::Warning))?;
            return Ok(());
        }
    };
    if enabled && !super::codex_fast::supports_fast_mode(ctx.model()?.as_ref()) {
        ui.notify(&"Daybreak requires a Codex provider".into(), Some(NotifyType::Warning))?;
        return Ok(());
    }
    write_state(state, enabled);
    pi.append_entry(
        &DAYBREAK_ENTRY.into(),
        Some(pi_js_compat::js_json!({"cyber": if enabled { Some(DAYBREAK_BLUE) } else { None }})),
    )?;
    update_status(ctx, enabled, ctx.model()?.as_ref())?;
    notify_selection(ctx, enabled)
}

fn apply_request(event: JsValue, ctx: &ExtensionContext, enabled: bool) -> Result<Option<JsValue>, HandlerError> {
    if !enabled || is_child_runtime(ctx)? || !super::codex_fast::supports_fast_mode(ctx.model()?.as_ref()) {
        return Ok(None);
    }
    let JsValue::Object(mut payload) = event else {
        ctx.ui()?.notify(
            &"Daybreak skipped: provider payload is not an object".into(),
            Some(NotifyType::Warning),
        )?;
        return Ok(None);
    };
    let eligible = payload
        .get("model")
        .and_then(JsValue::as_string)
        .is_some_and(|model| BLUE_ELIGIBLE_MODELS.iter().any(|id| model == *id));
    if !eligible {
        return Ok(None);
    }
    let mut access = payload
        .get("access_programs")
        .and_then(JsValue::as_object)
        .cloned()
        .unwrap_or_default();
    access.insert("cyber".into(), JsValue::String(DAYBREAK_BLUE.into()));
    payload.insert("access_programs".into(), JsValue::Object(access));
    Ok(Some(JsValue::Object(payload)))
}

pub fn codex_daybreak_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state: SharedState = Arc::new(Mutex::new(false));
    pi.register_command(
        "daybreak",
        Some("Select the OpenAI Daybreak Blue access program for eligible Codex models in this session".into()),
        None,
        {
            let state = state.clone();
            let pi = pi.clone();
            Arc::new(move |args, ctx| {
                let state = state.clone();
                let pi = pi.clone();
                Box::pin(async move {
                    apply_command(&args, &ctx, &pi, &state)?;
                    ctx.ui()?.set_editor_text(&JsString::default())?;
                    Ok(())
                })
            })
        },
    )
    .map_err(HandlerError::new)?;
    pi.on_session_start({
        let state = state.clone();
        move |_event, ctx| {
            let state = state.clone();
            Box::pin(async move {
                if is_child_runtime(&ctx)? {
                    return Ok(None);
                }
                let enabled = read_persisted_program(&ctx)?;
                write_state(&state, enabled);
                update_status(&ctx, enabled, ctx.model()?.as_ref())?;
                Ok(None)
            }) as pi_ai::request::BoxFuture<'static, _>
        }
    })
    .map_err(HandlerError::new)?;
    pi.on_model_select({
        let state = state.clone();
        move |event: crate::core::extensions::types::ModelSelectEvent, ctx| {
            let enabled = read_state(&state);
            Box::pin(async move {
                update_status(&ctx, enabled, Some(&event.model))?;
                Ok(None)
            }) as pi_ai::request::BoxFuture<'static, _>
        }
    })
    .map_err(HandlerError::new)?;
    pi.on_before_provider_request(
        move |event: crate::core::extensions::types::BeforeProviderRequestEvent, ctx| {
            let enabled = read_state(&state);
            Box::pin(async move { apply_request(event.payload, &ctx, enabled) })
                as pi_ai::request::BoxFuture<'static, _>
        },
    )
    .map_err(HandlerError::new)
}
