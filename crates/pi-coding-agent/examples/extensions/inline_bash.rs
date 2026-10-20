//! Opt-in native inline-bash example. Commands execute in the explicitly loaded cwd.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::string::{js_trim, replace, utf16_slice_to_utf8};

pub fn inline_bash_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    let pattern = regex::Regex::new(r"!\{([^}]+)\}").expect("constant inline pattern");
    pi.on_input(
        move |event: InputEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>> {
            let api = api.clone();
            let pattern = pattern.clone();
            Box::pin(async move {
                let start = event
                    .text
                    .trim_start_matches(pi_js_compat::string::is_js_whitespace_char);
                if start.starts_with('!') && !start.starts_with("!{") {
                    return Ok(Some(InputEventResult::Continue));
                }
                let matches: Vec<_> = pattern
                    .captures_iter(&event.text)
                    .map(|m| (m[0].to_owned(), m[1].to_owned()))
                    .collect();
                if matches.is_empty() {
                    return Ok(Some(InputEventResult::Continue));
                }
                let mut result = event.text;
                let mut summaries = Vec::new();
                for (full, command) in matches {
                    let (output, error) = match api
                        .exec(
                            "bash".into(),
                            vec!["-c".into(), command.clone()],
                            Some(ExtensionExecOptions {
                                timeout: Some(30_000),
                                ..Default::default()
                            }),
                        )
                        .await
                    {
                        Ok(value) => {
                            let output = if value.stdout.is_empty() {
                                &value.stderr
                            } else {
                                &value.stdout
                            };
                            let error = (value.code != 0 && !value.stderr.is_empty())
                                .then(|| format!("exit code {}", value.code));
                            (js_trim(output).to_owned(), error)
                        }
                        Err(error) => {
                            result = replace(&result, &full, &format!("[error: {error}]"));
                            summaries.push(format!("!{{{command}}} ({error}) -> \"\""));
                            continue;
                        }
                    };
                    result = replace(&result, &full, &output);
                    let preview = if output.encode_utf16().count() > 50 {
                        format!("{}...", utf16_slice_to_utf8(&output, 0, 50))
                    } else {
                        output
                    };
                    let status = error.map(|error| format!(" ({error})")).unwrap_or_default();
                    summaries.push(format!("!{{{command}}}{status} -> \"{preview}\""));
                }
                if ctx.has_ui()? {
                    ctx.ui()?.notify(
                        &format!(
                            "Expanded {} inline command(s):\n{}",
                            summaries.len(),
                            summaries.join("\n")
                        ),
                        Some(NotifyType::Info),
                    );
                }
                Ok(Some(InputEventResult::Transform {
                    text: result,
                    images: event.images,
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}
