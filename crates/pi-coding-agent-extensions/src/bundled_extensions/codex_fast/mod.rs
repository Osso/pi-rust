//! Bundled `codex-fast` extension (`packages/coding-agent/extensions/codex-fast/src/index.ts`).

use std::sync::{Arc, Mutex};

use pi_ai::types::Model;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};

use crate::core::extensions::types::{
    BeforeProviderRequestEvent, BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError,
    ModelSelectEvent, NotifyType,
};
use crate::core::session_manager::{KnownSessionEntry, lock_session_manager};

const FAST_MODE_ENTRY: &str = "codex-fast-mode";
const FAST_STATUS_KEY: &str = "codex-fast";
const PRIORITY_SERVICE_TIER: &str = "priority";
const ULTRAFAST_SERVICE_TIER: &str = "ultrafast";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastServiceTier {
    Priority,
    Ultrafast,
}

impl FastServiceTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Priority => PRIORITY_SERVICE_TIER,
            Self::Ultrafast => ULTRAFAST_SERVICE_TIER,
        }
    }
}

/// Shared main-thread fast-mode authority (TS `FastModeAuthority`).
#[derive(Debug, Clone, Default)]
pub struct FastModeAuthority {
    service_tier: Arc<Mutex<Option<FastServiceTier>>>,
}

impl FastModeAuthority {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn service_tier(&self) -> Option<FastServiceTier> {
        *self
            .service_tier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_service_tier(&self, service_tier: Option<FastServiceTier>) {
        *self
            .service_tier
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = service_tier;
    }
}

#[derive(Debug, Clone, Default)]
pub struct CodexFastOptions {
    pub authority: FastModeAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestedFastMode {
    Off,
    Tier(FastServiceTier),
}

pub fn supports_fast_mode(model: Option<&Model>) -> bool {
    model.is_some_and(|model| {
        matches!(
            model.provider.as_str(),
            "openai-codex" | "openai-codex-gc" | "openai-codex-team"
        )
    })
}

pub fn requested_fast_mode(
    args: impl Into<JsString>,
    current_tier: Option<FastServiceTier>,
) -> Option<RequestedFastMode> {
    let requested = args.into().trim().to_lowercase();
    if requested.is_empty() {
        return Some(match current_tier {
            None => RequestedFastMode::Tier(FastServiceTier::Priority),
            Some(_) => RequestedFastMode::Off,
        });
    }
    match &requested {
        value if value == "on" => Some(RequestedFastMode::Tier(FastServiceTier::Priority)),
        value if value == "ultra" => Some(RequestedFastMode::Tier(FastServiceTier::Ultrafast)),
        value if value == "off" => Some(RequestedFastMode::Off),
        _ => None,
    }
}

pub fn fast_mode_label(service_tier: Option<FastServiceTier>) -> &'static str {
    match service_tier {
        Some(FastServiceTier::Ultrafast) => "ultra",
        Some(FastServiceTier::Priority) => "on",
        None => "off",
    }
}

fn is_child_runtime(ctx: &ExtensionContext) -> Result<bool, HandlerError> {
    Ok(ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()?)
}

fn update_fast_status(
    ctx: &ExtensionContext,
    authority: &FastModeAuthority,
    model: Option<&Model>,
) -> Result<(), HandlerError> {
    let authority = authority.clone();
    let model = model.cloned();
    let status = crate::core::footer_data_provider::ExtensionStatus::Callback(Arc::new(move |view| {
        let displayed_model = match view.as_ref() {
            Some(view) => view.model.as_ref(),
            None => model.as_ref(),
        };
        let tier = authority.service_tier();
        if !supports_fast_mode(displayed_model) || tier.is_none() {
            return Ok(None);
        }
        let text = match tier {
            Some(FastServiceTier::Ultrafast) => "fast ultra",
            _ => "fast",
        };
        Ok(Some(text.into()))
    }));
    ctx.ui()?.set_status(&FAST_STATUS_KEY.into(), Some(status))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistedServiceTier {
    Null,
    Tier(FastServiceTier),
}

fn parse_persisted_service_tier(data: &JsValue) -> Option<PersistedServiceTier> {
    let object = data.as_object()?;
    match object.get("serviceTier") {
        Some(JsValue::Null) => Some(PersistedServiceTier::Null),
        Some(JsValue::String(value)) if value == PRIORITY_SERVICE_TIER => {
            Some(PersistedServiceTier::Tier(FastServiceTier::Priority))
        }
        Some(JsValue::String(value)) if value == ULTRAFAST_SERVICE_TIER => {
            Some(PersistedServiceTier::Tier(FastServiceTier::Ultrafast))
        }
        _ => None,
    }
}

pub fn read_persisted_fast_mode(ctx: &ExtensionContext) -> Result<Option<PersistedServiceTier>, HandlerError> {
    let session_manager = ctx.session_manager()?;
    let branch = lock_session_manager(&session_manager).get_branch(None);
    for entry in branch.iter().rev() {
        let known = entry.known();
        let Some(KnownSessionEntry::Custom(custom)) = known.as_deref() else {
            continue;
        };
        if custom.custom_type != FAST_MODE_ENTRY {
            continue;
        }
        let data = custom.data.clone().unwrap_or(JsValue::Null);
        if let Some(tier) = parse_persisted_service_tier(&data) {
            return Ok(Some(tier));
        }
    }
    Ok(None)
}

pub fn read_configured_fast_mode(ctx: &ExtensionContext) -> Result<Option<FastServiceTier>, HandlerError> {
    let Some(settings) = ctx.settings_manager()? else {
        return Ok(None);
    };
    let merged = settings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_merged_settings();
    Ok(configured_service_tier(&merged))
}

fn configured_service_tier(settings: &JsObject) -> Option<FastServiceTier> {
    match settings.get("defaultCodexFastMode") {
        Some(JsValue::String(value)) if value == PRIORITY_SERVICE_TIER => Some(FastServiceTier::Priority),
        Some(JsValue::String(value)) if value == ULTRAFAST_SERVICE_TIER => Some(FastServiceTier::Ultrafast),
        _ => None,
    }
}

fn persist_fast_mode(pi: &ExtensionApi, authority: &FastModeAuthority) -> Result<(), HandlerError> {
    let mut data = JsObject::new();
    data.insert(
        "serviceTier".into(),
        match authority.service_tier() {
            Some(tier) => JsValue::String(tier.as_str().into()),
            None => JsValue::Null,
        },
    );
    pi.append_entry(&FAST_MODE_ENTRY.into(), Some(JsValue::Object(data)))
}

pub async fn handle_fast_command(
    args: impl Into<JsString>,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    authority: &FastModeAuthority,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if is_child_runtime(ctx)? {
        ui.notify(
            &"Fast mode is controlled by the main thread".into(),
            Some(NotifyType::Warning),
        )?;
        ui.set_editor_text(&JsString::default())?;
        return Ok(());
    }
    let Some(requested) = requested_fast_mode(args, authority.service_tier()) else {
        ui.notify(&"Usage: /fast [on|off|ultra]".into(), Some(NotifyType::Warning))?;
        ui.set_editor_text(&JsString::default())?;
        return Ok(());
    };
    if let RequestedFastMode::Tier(_) = requested
        && !supports_fast_mode(ctx.model()?.as_ref())
    {
        ui.notify(&"Fast mode requires a Codex provider".into(), Some(NotifyType::Warning))?;
        ui.set_editor_text(&JsString::default())?;
        return Ok(());
    }

    authority.set_service_tier(match requested {
        RequestedFastMode::Off => None,
        RequestedFastMode::Tier(tier) => Some(tier),
    });
    persist_fast_mode(pi, authority)?;
    update_fast_status(ctx, authority, ctx.model()?.as_ref())?;
    ui.notify(
        &format!("Fast mode: {}", fast_mode_label(authority.service_tier())).into(),
        Some(NotifyType::Info),
    )?;
    ui.set_editor_text(&JsString::default())?;
    Ok(())
}

/// Bundled `codex-fast` extension.
pub fn codex_fast_extension(pi: &ExtensionApi, options: &CodexFastOptions) -> Result<(), HandlerError> {
    let authority = options.authority.clone();
    pi.register_command(
        "fast",
        Some("Toggle Codex priority or ultrafast processing from the main thread".into()),
        None,
        {
            let authority = authority.clone();
            let pi = pi.clone();
            Arc::new(move |args, ctx| {
                let authority = authority.clone();
                let pi = pi.clone();
                Box::pin(async move { handle_fast_command(&args, &ctx, &pi, &authority).await })
            })
        },
    )
    .map_err(HandlerError::new)?;

    pi.on_session_start({
        let authority = authority.clone();
        move |_event, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let authority = authority.clone();
            Box::pin(async move {
                if !is_child_runtime(&ctx)? {
                    let persisted = read_persisted_fast_mode(&ctx)?;
                    authority.set_service_tier(match persisted {
                        None => read_configured_fast_mode(&ctx)?,
                        Some(PersistedServiceTier::Null) => None,
                        Some(PersistedServiceTier::Tier(tier)) => Some(tier),
                    });
                }
                update_fast_status(&ctx, &authority, ctx.model()?.as_ref())?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.on_model_select({
        let authority = authority.clone();
        move |event: ModelSelectEvent, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let authority = authority.clone();
            Box::pin(async move {
                update_fast_status(&ctx, &authority, Some(&event.model))?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.on_before_provider_request({
        let authority = authority.clone();
        move |event: BeforeProviderRequestEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<JsValue>, HandlerError>> {
            let authority = authority.clone();
            Box::pin(async move {
                let Some(tier) = authority.service_tier() else {
                    return Ok(None);
                };
                if !supports_fast_mode(ctx.model()?.as_ref()) {
                    return Ok(None);
                }
                let JsValue::Object(mut payload) = event.payload else {
                    ctx.ui()?.notify(
                        &"Fast mode skipped: provider payload is not an object".into(),
                        Some(NotifyType::Warning),
                    )?;
                    return Ok(None);
                };
                payload.insert("service_tier".into(), JsValue::String(tier.as_str().into()));
                Ok(Some(JsValue::Object(payload)))
            })
        }
    })
    .map_err(HandlerError::new)?;
    Ok(())
}

/// Registers the extension factory under `<first-party:codex-fast>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry, authority: FastModeAuthority) {
    registry.register(
        "<first-party:codex-fast>",
        Arc::new(move |pi| {
            let options = CodexFastOptions {
                authority: authority.clone(),
            };
            let result = codex_fast_extension(pi, &options);
            Box::pin(async move { result })
        }),
    );
}
