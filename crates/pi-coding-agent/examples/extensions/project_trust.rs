//! Opt-in native `project-trust.ts`; runs before session initialization.
use pi_coding_agent::core::extensions::*;

pub fn project_trust_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_project_trust(
        |event: ProjectTrustEvent,
         ctx: ProjectTrustContext|
         -> BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>> {
            Box::pin(async move {
                let mode = serde_json::to_value(ctx.mode).map_err(|e| HandlerError::new(e.to_string()))?;
                ctx.ui.notify(
                    &format!(
                        "project_trust fired for {} (mode: {}, load: 1)",
                        event.cwd,
                        mode.as_str().unwrap()
                    ),
                    Some(NotifyType::Info),
                );
                let undecided = ProjectTrustEventResult {
                    trusted: ProjectTrustEventDecision::Undecided,
                    remember: None,
                };
                if !ctx.has_ui {
                    return Ok(undecided);
                }
                let choices = [
                    "Trust and remember",
                    "Trust with note and remember",
                    "Trust this session",
                    "Do not trust this session",
                    "Let built-in prompt decide",
                ]
                .map(str::to_owned);
                let choice = ctx
                    .ui
                    .select(&format!("Project trust for:\n{}", event.cwd), &choices, None)
                    .await?;
                let result = match choice.as_deref() {
                    Some("Trust with note and remember") => {
                        let note = ctx
                            .ui
                            .input("Project trust note", Some("Optional note for this demo"), None)
                            .await?;
                        let message = note
                            .filter(|n| !n.is_empty())
                            .map_or_else(|| "No demo note entered".into(), |n| format!("Recorded demo note: {n}"));
                        ctx.ui.notify(&message, Some(NotifyType::Info));
                        ProjectTrustEventResult {
                            trusted: ProjectTrustEventDecision::Yes,
                            remember: Some(true),
                        }
                    }
                    Some("Trust and remember") => ProjectTrustEventResult {
                        trusted: ProjectTrustEventDecision::Yes,
                        remember: Some(true),
                    },
                    Some("Trust this session") => ProjectTrustEventResult {
                        trusted: ProjectTrustEventDecision::Yes,
                        remember: None,
                    },
                    Some("Do not trust this session") => ProjectTrustEventResult {
                        trusted: ProjectTrustEventDecision::No,
                        remember: None,
                    },
                    _ => undecided,
                };
                Ok(result)
            })
        },
    )?;
    pi.on_session_start(
        |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                ctx.ui()?.notify(
                    &format!("project-trust example loaded after trust resolution in {}", ctx.cwd()?),
                    Some(NotifyType::Info),
                );
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
