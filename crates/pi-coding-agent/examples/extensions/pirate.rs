//! Opt-in native `pirate.ts`; each explicit factory owns its toggle state.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use pi_coding_agent::core::extensions::*;

const PIRATE_PROMPT: &str = "\n\nIMPORTANT: You are now in PIRATE MODE. You must:
- Speak like a stereotypical pirate in all responses
- Use phrases like \"Arrr!\", \"Ahoy!\", \"Shiver me timbers!\", \"Avast!\", \"Ye scurvy dog!\"
- Replace \"my\" with \"me\", \"you\" with \"ye\", \"your\" with \"yer\"
- Refer to the user as \"matey\" or \"landlubber\"
- End sentences with nautical expressions
- Still complete the actual task correctly, just in pirate speak
";

pub fn pirate_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let mode = Arc::new(AtomicBool::new(false));
    let toggle = mode.clone();
    pi.register_command(
        "pirate",
        Some("Toggle pirate mode (agent speaks like a pirate)".into()),
        None,
        Arc::new(move |_args, ctx| {
            let enabled = !toggle.fetch_xor(true, Ordering::SeqCst);
            Box::pin(async move {
                ctx.ui()?.notify(
                    if enabled {
                        "Arrr! Pirate mode enabled!"
                    } else {
                        "Pirate mode disabled"
                    },
                    Some(NotifyType::Info),
                );
                Ok(())
            })
        }),
    )?;
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              _ctx|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let enabled = mode.load(Ordering::SeqCst);
            Box::pin(async move {
                Ok(enabled.then(|| BeforeAgentStartEventResult {
                    system_prompt: Some(format!("{}{PIRATE_PROMPT}", event.system_prompt)),
                    message: None,
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}
