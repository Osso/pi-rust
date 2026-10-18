//! Resident console command (`cli/resident-console-command.ts`).
//!
//! `parseResidentConsoleArgs` is ported and tested. `runResidentConsoleCommand`
//! and `ResidentConsoleUi` are blocked on `AgentSessionEvent`
//! (`core/agent-session.ts`) and the `AssistantMessageComponent` /
//! `UserMessageComponent` components.

use crate::core::resident_console_transport::ResidentConsoleService;

/// `ResidentConsoleCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentConsoleCommand {
    pub service: ResidentConsoleService,
    pub initial_prompt: Option<String>,
}

impl ResidentConsoleService {
    /// CLI flag / socket suffix for the service.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Architect => "architect",
            Self::Supervisor => "supervisor",
        }
    }
}

/// `parseResidentConsoleArgs`.
///
/// `Ok(None)` when the first argument is not `--supervisor`/`--architect`;
/// `Err` is the thrown flag-conflict error.
pub fn parse_resident_console_args(args: &[String]) -> Result<Option<ResidentConsoleCommand>, String> {
    let Some(raw_service) = args.first() else {
        return Ok(None);
    };
    let service = match raw_service.as_str() {
        "--supervisor" => ResidentConsoleService::Supervisor,
        "--architect" => ResidentConsoleService::Architect,
        _ => return Ok(None),
    };
    let prompt_parts = &args[1..];
    if prompt_parts.iter().any(|part| part.starts_with('-')) {
        return Err(format!(
            "--{} cannot be combined with normal Pi CLI flags",
            service.as_str()
        ));
    }
    let initial_prompt = {
        let joined = prompt_parts.join(" ");
        let trimmed = joined.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    };
    Ok(Some(ResidentConsoleCommand {
        service,
        initial_prompt,
    }))
}
