//! Opt-in native GitHub issue autocomplete. Executes git/gh only when explicitly loaded.
use futures_util::{FutureExt, future::Shared};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::js_trim;
use pi_tui::autocomplete::*;
use serde::Deserialize;
use std::{rc::Rc, sync::Arc};

#[derive(Clone, Deserialize)]
struct Issue {
    number: f64,
    title: String,
    state: String,
}
type Issues = Shared<BoxFuture<'static, Result<Option<Vec<Issue>>, pi_js_compat::callback_error::CallbackError>>>;
struct Provider {
    current: Rc<dyn AutocompleteProvider>,
    issues: Issues,
}

pub fn github_issue_autocomplete_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let api = pi.clone();
    pi.on_session_start(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            Box::pin(async move {
                let ui = ctx.ui()?;
                let result = api
                    .exec(
                        "git".into(),
                        vec!["remote".into(), "-v".into()],
                        Some(exec_options(&ctx.cwd()?)),
                    )
                    .await?;
                if result.code != 0 {
                    ui.notify(
                        &JsString::from("github-issue-autocomplete: cwd is not a git repository"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(None);
                }
                let repo = result
                    .stdout
                    .lines()
                    .find_map(|line| line.split_whitespace().nth(1).and_then(parse_repo));
                let Some(repo) = repo else {
                    ui.notify(
                        &JsString::from("github-issue-autocomplete: cwd is not a GitHub repository"),
                        Some(NotifyType::Error),
                    )?;
                    return Ok(None);
                };
                let cwd = ctx.cwd()?;
                let notify = ui.clone();
                let issues: Issues = async move { load_issues(api, cwd, repo, notify).await }
                    .boxed()
                    .shared();
                let preload = issues.clone();
                tokio::spawn(async move {
                    // Same eager one-shot promise as the source; requests share its outcome.
                    // Frozen github-issue-autocomplete.ts:183 leaves the eager promise uncaught.
                    preload
                        .await
                        .unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
                });
                ui.add_autocomplete_provider(Arc::new(move |current| {
                    Rc::new(Provider {
                        current,
                        issues: issues.clone(),
                    })
                }));
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
fn exec_options(cwd: &JsString) -> ExtensionExecOptions {
    ExtensionExecOptions {
        cwd: Some(cwd.into()),
        timeout: Some(5_000),
        ..Default::default()
    }
}
fn parse_repo(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(path.into())
}
async fn load_issues(
    api: ExtensionApi,
    cwd: JsString,
    repo: String,
    ui: Arc<dyn ExtensionUiContext>,
) -> Result<Option<Vec<Issue>>, pi_js_compat::callback_error::CallbackError> {
    let args = [
        "issue",
        "list",
        "--repo",
        &repo,
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        "number,title,state",
    ]
    .map(str::to_owned)
    .to_vec();
    let result = api
        .exec("gh".into(), args, Some(exec_options(&JsString::from(&cwd))))
        .await?;
    if result.code != 0 {
        let stderr = js_trim(&result.stderr);
        let details = if stderr.is_empty() {
            format!("exit code {}", result.code)
        } else {
            stderr.into()
        };
        ui.notify(
            &JsString::from(format!("github-issue-autocomplete: failed to load issues: {details}")),
            Some(NotifyType::Error),
        )?;
        return Ok(None);
    }
    // Decode JSON before projecting issue fields into the String-typed fuzzy API.
    let parsed = pi_js_compat::js_json::parse(&result.stdout)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok());
    match parsed {
        Some(issues) => Ok(Some(issues)),
        None => {
            ui.notify(
                &JsString::from("github-issue-autocomplete: failed to parse gh issue list output"),
                Some(NotifyType::Error),
            )?;
            Ok(None)
        }
    }
}
fn issue_number(issue: &Issue) -> String {
    pi_js_compat::number::js_number_to_string(issue.number)
}
fn format_item(issue: &Issue) -> AutocompleteItem {
    let value = format!("#{}", issue_number(issue));
    AutocompleteItem {
        value: JsString::from(value.clone()),
        label: JsString::from(value),
        description: Some(JsString::from(format!(
            "[{}] {}",
            issue.state.to_lowercase(),
            issue.title
        ))),
    }
}
fn filter(issues: &[Issue], query: &JsString) -> Vec<AutocompleteItem> {
    if query.trim().is_empty() {
        return issues.iter().take(20).map(format_item).collect();
    }
    if query
        .units()
        .iter()
        .all(|unit| (b'0' as u16..=b'9' as u16).contains(unit))
    {
        let matches: Vec<_> = issues
            .iter()
            .filter(|issue| JsString::from(issue_number(issue)).starts_with(query))
            .take(20)
            .map(format_item)
            .collect();
        if !matches.is_empty() {
            return matches;
        }
    }
    pi_tui::fuzzy::fuzzy_filter(issues, query, |issue| {
        vec![format!("{} {}", issue_number(issue), issue.title)]
    })
    .into_iter()
    .take(20)
    .map(|issue| format_item(&issue))
    .collect()
}
fn token(text: &JsString) -> Option<JsString> {
    pi_js_compat::js_regex::JsRegex::new(r"(?:^|[ \t])#([^\s#]*)$")
        .expect("constant issue token pattern")
        .captures(text)?
        .get(1)
        .map(|capture| capture.text())
}

impl AutocompleteProvider for Provider {
    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<JsString>,
        line: usize,
        col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture {
        Box::pin(async move {
            let text = lines.get(line).cloned().unwrap_or_else(JsString::default);
            if let Some(query) = token(&text.slice(0, col)) {
                let issues = self
                    .issues
                    .clone()
                    .await
                    .unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
                if !options.signal.aborted()
                    && let Some(issues) = issues
                {
                    let items = filter(&issues, &query);
                    if !items.is_empty() {
                        return Some(AutocompleteSuggestions {
                            items,
                            prefix: pi_js_compat::js_format!("#{}", query),
                        });
                    }
                }
            }
            self.current
                .clone()
                .get_suggestions(lines.into_iter().collect::<Vec<_>>(), line, col, options)
                .await
        })
    }
    fn apply_completion(
        &self,
        lines: &[JsString],
        line: usize,
        col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult {
        self.current
            .apply_completion(lines, line, col, item, &JsString::from(prefix))
    }
    fn should_trigger_file_completion(&self, lines: &[JsString], line: usize, col: usize) -> Option<bool> {
        Some(
            self.current
                .should_trigger_file_completion(lines, line, col)
                .unwrap_or(true),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_lone_surrogates_project_to_replacement_at_string_fuzzy_boundary() {
        let raw = r#"[{"number":77,"title":"Lone \ud800 and \udfff","state":"OPEN"}]"#;
        let decoded = pi_js_compat::js_json::parse(raw).unwrap();
        let issues: Vec<Issue> = serde_json::from_value(decoded).unwrap();
        assert_eq!(issues[0].title, "Lone � and �");

        // compatibility.md:177 excludes lone-surrogate fidelity in String core APIs.
        let matches = filter(&issues, &JsString::from("�"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, "#77");
        assert_eq!(matches[0].label, "#77");
        assert_eq!(
            matches[0].description.as_ref(),
            Some(&JsString::from("[open] Lone � and �"))
        );
    }
}
