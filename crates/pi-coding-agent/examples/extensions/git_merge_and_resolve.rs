//! Opt-in native merge-and-resolve example. Uses the host's execution cwd.
use pi_coding_agent::core::exec::ExecResult;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use pi_js_compat::string::js_trim;

struct ConflictBlock {
    file: String,
    start: usize,
    separator: usize,
    end: usize,
}

async fn git(api: &ExtensionApi, args: &[&str]) -> Result<ExecResult, HandlerError> {
    api.exec("git".into(), args.iter().map(|arg| (*arg).into()).collect(), None)
        .await
        .map_err(HandlerError::new)
}

async fn find_conflicts(api: &ExtensionApi, cwd: &JsString) -> Result<Vec<ConflictBlock>, HandlerError> {
    let result = git(api, &["diff", "--name-only", "--diff-filter=U"]).await?;
    if result.code != 0 || js_trim(&result.stdout).is_empty() {
        return Ok(Vec::new());
    }
    let mut blocks = Vec::new();
    for file in js_trim(&result.stdout).split('\n') {
        // The pinned example ignores per-file read failures.
        let Ok(contents) = tokio::fs::read(std::path::Path::new(&encode_node_utf8(cwd)).join(file)).await else {
            continue;
        };
        let mut start = None;
        let mut separator = None;
        // Node readline splits CR, LF and CRLF; unlike str::lines it also splits lone CR.
        let text = String::from_utf8_lossy(&contents)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        for (index, line) in text.split_terminator('\n').enumerate() {
            let number = index + 1;
            if line.starts_with("<<<<<<<") {
                start = Some(number);
                separator = None;
            } else if line.starts_with("=======") && start.is_some() {
                separator = Some(number);
            } else if line.starts_with(">>>>>>>")
                && let (Some(first), Some(middle)) = (start, separator)
            {
                blocks.push(ConflictBlock {
                    file: file.into(),
                    start: first,
                    separator: middle,
                    end: number,
                });
                start = None;
                separator = None;
            }
        }
    }
    Ok(blocks)
}
fn format_range(start: usize, end: usize) -> String {
    if start > end {
        "empty".into()
    } else if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}
fn format_conflicts(reference: &str, blocks: &[ConflictBlock]) -> String {
    let mut lines = vec![format!("Merged {reference} with conflicts:"), String::new()];
    for block in blocks {
        lines.push(format!(
            "  {}:{}-{} (ours {}, theirs {})",
            block.file,
            block.start,
            block.end,
            format_range(block.start + 1, block.separator - 1),
            format_range(block.separator + 1, block.end - 1)
        ));
    }
    lines.extend([String::new(), "Resolve these conflicts.".into()]);
    lines.join("\n")
}

pub fn git_merge_and_resolve_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.on_agent_end(
        move |_: AgentEndEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            Box::pin(async move {
                if git(&api, &["rev-parse", "--git-dir"]).await?.code != 0 {
                    return Ok(None);
                }
                let mut reference = "MERGE_HEAD".to_owned();
                if git(&api, &["rev-parse", "MERGE_HEAD"]).await?.code != 0 {
                    if !js_trim(&git(&api, &["status", "--porcelain"]).await?.stdout).is_empty() {
                        return Ok(None);
                    }
                    let upstream = git(&api, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).await?;
                    if upstream.code != 0 {
                        return Ok(None);
                    }
                    reference = js_trim(&upstream.stdout).into();
                    let remote = reference.split('/').next().unwrap_or("");
                    ctx.ui()?.notify(
                        &JsString::from(format!("git-merge-and-resolve: fetching {remote}, merging {reference}")),
                        Some(NotifyType::Info),
                    )?;
                    let fetch = git(&api, &["fetch", remote]).await?;
                    if fetch.code != 0 {
                        ctx.ui()?.notify(
                            &JsString::from(format!(
                                "git-merge-and-resolve: fetch failed: {}",
                                js_trim(&fetch.stderr)
                            )),
                            Some(NotifyType::Warning),
                        )?;
                        return Ok(None);
                    }
                    if git(&api, &["merge", "--no-ff", &reference]).await?.code == 0 {
                        return Ok(None);
                    }
                }
                let conflicts = find_conflicts(&api, &ctx.cwd()?).await?;
                if !conflicts.is_empty() {
                    api.send_user_message(
                        UserMessageContent::Text(JsString::from(format_conflicts(&reference, &conflicts))),
                        Some(SendUserMessageOptions {
                            deliver_as: Some(UserMessageDelivery::FollowUp),
                            ..Default::default()
                        }),
                    )?;
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
