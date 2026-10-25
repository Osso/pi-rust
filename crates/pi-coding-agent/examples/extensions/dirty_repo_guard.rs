//! Opt-in native `dirty-repo-guard.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::js_trim;

pub fn dirty_repo_guard_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.on_session_before_switch(move |event: SessionBeforeSwitchEvent, ctx: ExtensionContext| {
        let api = api.clone();
        Box::pin(async move {
            let action = if event.reason == SessionSwitchReason::New {
                "new session"
            } else {
                "switch session"
            };
            Ok(check_dirty_repo(&api, &ctx, action)
                .await?
                .then_some(SessionBeforeSwitchResult { cancel: Some(true) }))
        }) as BoxFuture<'static, Result<Option<SessionBeforeSwitchResult>, HandlerError>>
    })?;
    let api = pi.clone();
    pi.on_session_before_fork(move |_event, ctx: ExtensionContext| {
        let api = api.clone();
        Box::pin(async move {
            Ok(check_dirty_repo(&api, &ctx, "fork")
                .await?
                .then_some(SessionBeforeForkResult {
                    cancel: Some(true),
                    ..Default::default()
                }))
        }) as BoxFuture<'static, Result<Option<SessionBeforeForkResult>, HandlerError>>
    })
    .map_err(HandlerError::new)
}
async fn check_dirty_repo(pi: &ExtensionApi, ctx: &ExtensionContext, action: &str) -> Result<bool, HandlerError> {
    let result = pi
        .exec("git".into(), vec!["status".into(), "--porcelain".into()], None)
        .await?;
    let changes = js_trim(&result.stdout);
    if result.code != 0 || changes.is_empty() {
        return Ok(false);
    }
    if !ctx.has_ui()? {
        return Ok(true);
    }
    let count = changes.split('\n').filter(|line| !line.is_empty()).count();
    let ui = ctx.ui()?;
    let choice = ui
        .select(
            &JsString::from(format!("You have {count} uncommitted file(s). {action} anyway?")),
            &["Yes, proceed anyway".into(), "No, let me commit first".into()],
            None,
        )
        .await?;
    if choice.as_ref() == Some(&JsString::from("Yes, proceed anyway")) {
        return Ok(false);
    }
    ui.notify(&JsString::from("Commit your changes first"), Some(NotifyType::Warning))?;
    Ok(true)
}
