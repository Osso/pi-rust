//! Bundled `codex-fast` extension (`packages/coding-agent/extensions/codex-fast/src/index.ts`).

use std::sync::{Arc, Mutex};

use pi_ai::types::Model;
use serde_json::{Map, Value};

use crate::core::extensions::types::{
    BeforeProviderRequestEvent, BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError,
    ModelSelectEvent, NotifyType,
};
use crate::core::session_manager::{KnownSessionEntry, SessionEntry};

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
    model.is_some_and(|model| model.provider == "openai-codex" || model.provider == "openai-codex-gc")
}

pub fn requested_fast_mode(args: &str, current_tier: Option<FastServiceTier>) -> Option<RequestedFastMode> {
    let requested = pi_js_compat::string::js_trim(args).to_lowercase();
    if requested.is_empty() {
        return Some(match current_tier {
            None => RequestedFastMode::Tier(FastServiceTier::Priority),
            Some(_) => RequestedFastMode::Off,
        });
    }
    match requested.as_str() {
        "on" => Some(RequestedFastMode::Tier(FastServiceTier::Priority)),
        "ultra" => Some(RequestedFastMode::Tier(FastServiceTier::Ultrafast)),
        "off" => Some(RequestedFastMode::Off),
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
    let service_tier = authority.service_tier();
    let label = fast_mode_label(service_tier);
    let status = if label == "ultra" { "fast ultra" } else { "fast" };
    let text = if service_tier.is_some() && supports_fast_mode(model) {
        Some(status)
    } else {
        None
    };
    ctx.ui()?.set_status(FAST_STATUS_KEY, text);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistedServiceTier {
    Null,
    Tier(FastServiceTier),
}

fn parse_persisted_service_tier(data: &Value) -> Option<PersistedServiceTier> {
    let object = data.as_object()?;
    match object.get("serviceTier") {
        Some(Value::Null) => Some(PersistedServiceTier::Null),
        Some(Value::String(value)) if value == PRIORITY_SERVICE_TIER => {
            Some(PersistedServiceTier::Tier(FastServiceTier::Priority))
        }
        Some(Value::String(value)) if value == ULTRAFAST_SERVICE_TIER => {
            Some(PersistedServiceTier::Tier(FastServiceTier::Ultrafast))
        }
        _ => None,
    }
}

pub fn read_persisted_fast_mode(ctx: &ExtensionContext) -> Result<Option<PersistedServiceTier>, HandlerError> {
    let branch = ctx.session_manager()?.get_branch(None);
    for entry in branch.iter().rev() {
        let SessionEntry::Known(KnownSessionEntry::Custom(custom)) = entry else {
            continue;
        };
        if custom.custom_type != FAST_MODE_ENTRY {
            continue;
        }
        let data = custom.data.clone().unwrap_or(Value::Null);
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

fn configured_service_tier(settings: &serde_json::Map<String, Value>) -> Option<FastServiceTier> {
    match settings.get("defaultCodexFastMode") {
        Some(Value::String(value)) if value == PRIORITY_SERVICE_TIER => Some(FastServiceTier::Priority),
        Some(Value::String(value)) if value == ULTRAFAST_SERVICE_TIER => Some(FastServiceTier::Ultrafast),
        _ => None,
    }
}

fn persist_fast_mode(pi: &ExtensionApi, authority: &FastModeAuthority) -> Result<(), HandlerError> {
    let mut data = Map::new();
    data.insert(
        "serviceTier".to_owned(),
        match authority.service_tier() {
            Some(tier) => Value::String(tier.as_str().to_owned()),
            None => Value::Null,
        },
    );
    pi.append_entry(FAST_MODE_ENTRY, Some(Value::Object(data)))
}

pub async fn handle_fast_command(
    args: &str,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    authority: &FastModeAuthority,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if is_child_runtime(ctx)? {
        ui.notify("Fast mode is controlled by the main thread", Some(NotifyType::Warning));
        ui.set_editor_text("");
        return Ok(());
    }
    let Some(requested) = requested_fast_mode(args, authority.service_tier()) else {
        ui.notify("Usage: /fast [on|off|ultra]", Some(NotifyType::Warning));
        ui.set_editor_text("");
        return Ok(());
    };
    if let RequestedFastMode::Tier(_) = requested
        && !supports_fast_mode(ctx.model()?.as_ref())
    {
        ui.notify(
            "Fast mode requires openai-codex or openai-codex-gc",
            Some(NotifyType::Warning),
        );
        ui.set_editor_text("");
        return Ok(());
    }

    authority.set_service_tier(match requested {
        RequestedFastMode::Off => None,
        RequestedFastMode::Tier(tier) => Some(tier),
    });
    persist_fast_mode(pi, authority)?;
    update_fast_status(ctx, authority, ctx.model()?.as_ref())?;
    ui.notify(
        &format!("Fast mode: {}", fast_mode_label(authority.service_tier())),
        Some(NotifyType::Info),
    );
    ui.set_editor_text("");
    Ok(())
}

/// Bundled `codex-fast` extension.
pub fn codex_fast_extension(pi: &ExtensionApi, options: &CodexFastOptions) -> Result<(), HandlerError> {
    let authority = options.authority.clone();
    pi.register_command(
        "fast",
        Some("Toggle Codex priority or ultrafast processing from the main thread".to_owned()),
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
              -> BoxFuture<'static, Result<Option<Value>, HandlerError>> {
            let authority = authority.clone();
            Box::pin(async move {
                if authority.service_tier().is_none() || !supports_fast_mode(ctx.model()?.as_ref()) {
                    return Ok(None);
                }
                let Value::Object(mut payload) = event.payload else {
                    ctx.ui()?.notify(
                        "Fast mode skipped: provider payload is not an object",
                        Some(NotifyType::Warning),
                    );
                    return Ok(None);
                };
                payload.insert(
                    "service_tier".to_owned(),
                    Value::String(
                        authority
                            .service_tier()
                            .expect("checked service tier")
                            .as_str()
                            .to_owned(),
                    ),
                );
                Ok(Some(Value::Object(payload)))
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
