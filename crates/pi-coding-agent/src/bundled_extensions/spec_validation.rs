//! Bundled `spec-validation` extension (`extensions/spec-validation/src/index.ts`).

use std::sync::Arc;

use crate::core::extensions::types::{ExtensionApi, HandlerError, UserMessageContent};

const SPEC_VALIDATION_PROMPT: &str = "Use the `spec-format` skill to validate every project spec separately.

Steps:

1. Find all Markdown specs under `docs/specs/` in the current project.
2. If `docs/specs/` does not exist or contains no Markdown files, report that clearly and stop.
3. For each spec file, validate it independently against the `spec-format` requirements:
   - Opening paragraph explains what the feature is and where source lives.
   - Sections appear in the expected order.
   - `What it must do` contains testable checkbox bullets.
   - Checked bullets have matching tests listed in `Tests asserting this spec`.
   - `How it works` links to wiki or architecture docs instead of duplicating implementation prose.
   - `Implementation inventory` lists source files with one-line roles.
   - `Known gaps (current cycle)` and `Out of scope` are explicit.
   - Guessed or inferred requirements are marked or omitted.
4. Produce one result block per spec with `PASS` or `FAIL`, file path, and concrete issues.
5. Do not edit files unless explicitly asked after the validation report.
";

pub fn spec_validation_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.register_command(
        "spec-validation",
        Some("Validate each docs/specs/*.md file separately".into()),
        None,
        Arc::new(move |_args, ctx| {
            let api = api.clone();
            Box::pin(async move {
                if !ctx.is_idle()? {
                    return Err(HandlerError::new("/spec-validation is blocked while a task is running"));
                }

                api.send_user_message(UserMessageContent::Text(SPEC_VALIDATION_PROMPT.to_owned().into()), None)?;
                ctx.ui()?.set_editor_text("");
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
