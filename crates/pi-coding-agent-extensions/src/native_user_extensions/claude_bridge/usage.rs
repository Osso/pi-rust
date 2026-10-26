//! Subscription usage HTTP, idle-grant refresh and native claimable event handlers.
use super::{account_profiles, provider::BridgeProvider};
use crate::core::extensions::types::{
    CustomMessageContent, CustomMessagePayload, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError,
    NotifyType,
};
use pi_ai::request::{BoxFuture, Fetch, HttpRequest};
use pi_js_compat::{
    abort::AbortSignal,
    js_json::{self, JsString, JsValue},
};
use std::path::PathBuf;
use std::sync::Arc;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const MAX_ATTEMPTS: usize = 3;
const MAX_WAIT_MS: f64 = 5000.0;
const REFRESH_ARGS: [&str; 12] = [
    "-p",
    "Reply: ok",
    "--model",
    "haiku",
    "--max-turns",
    "1",
    "--tools",
    "",
    "--no-session-persistence",
    "--setting-sources",
    "project",
    "--strict-mcp-config",
];

#[derive(Clone)]
pub(super) struct UsageRuntime {
    pub fetch: Arc<dyn Fetch>,
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
    pub random: Arc<dyn Fn() -> Result<f64, JsString> + Send + Sync>,
    pub sleep: Arc<dyn Fn(f64) -> BoxFuture<'static, ()> + Send + Sync>,
}
impl Default for UsageRuntime {
    fn default() -> Self {
        Self {
            fetch: pi_ai::http::default_fetch(),
            now: Arc::new(
                || match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => d.as_secs_f64() * 1000.0,
                    Err(e) => -e.duration().as_secs_f64() * 1000.0,
                },
            ),
            random: Arc::new(|| {
                let bits = getrandom::u64().map_err(|e| JsString::from(e.to_string()))? >> 11;
                Ok(bits as f64 / (1_u64 << 53) as f64)
            }),
            sleep: Arc::new(|delay| {
                Box::pin(async move {
                    tokio::time::sleep(std::time::Duration::from_secs_f64(delay / 1000.0)).await;
                })
            }),
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct UsageError {
    pub message: JsString,
    pub status: Option<u16>,
    unexpected: bool,
}
impl UsageError {
    fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            status: None,
            unexpected: false,
        }
    }
    fn unexpected() -> Self {
        Self {
            message: JsString::default(),
            status: None,
            unexpected: true,
        }
    }
    fn report_message(&self, generic: &str) -> JsString {
        if self.unexpected {
            generic.into()
        } else {
            self.message.clone()
        }
    }
}
#[derive(Clone)]
pub(super) struct UsageAccount {
    pub home: PathBuf,
    pub name: String,
    pub profile: Option<String>,
    pub config_dir: Option<PathBuf>,
}

fn retry_after_ms(value: &str, now: f64) -> Option<f64> {
    let number = pi_js_compat::js_json::to_js_number(&JsValue::String(value.into())).ok()?;
    if !value.trim().is_empty() && number.is_finite() && number >= 0.0 {
        return Some(number * 1000.0);
    }
    let deadline = pi_js_compat::date::date_parse(value);
    deadline.is_finite().then(|| (deadline - now).max(0.0))
}
fn http_error(status: u16, delay: Option<f64>, now: f64) -> UsageError {
    if status == 401 {
        return UsageError::new("Claude usage HTTP 401: current account OAuth token expired or rejected");
    }
    if status == 403 {
        return UsageError::new(
            "Claude usage HTTP 403: token not authorized to read usage (inference-only setup-token? full `claude auth login` grants user:profile)",
        );
    }
    let retry = delay
        .map(|d| format!("; retry after {} seconds", (d / 1000.0).ceil()))
        .unwrap_or_default();
    let deadline = delay
        .filter(|d| *d <= 8.64e15 - now)
        .and_then(|d| pi_js_compat::date::to_iso_string_value(now + d).ok())
        .map(|iso| format!(" ({iso})"))
        .unwrap_or_default();
    UsageError {
        message: format!("Claude usage HTTP {status}{retry}{deadline}").into(),
        status: Some(status),
        unexpected: false,
    }
}
async fn fetch_response(token: &JsString, runtime: &UsageRuntime) -> Option<pi_ai::request::HttpResponse> {
    let token = pi_ai::request::header_value(&(JsString::from("Bearer ") + token)).ok()?;
    let request = HttpRequest {
        method: "GET".into(),
        url: USAGE_URL.into(),
        headers: vec![
            ("Authorization".into(), token),
            ("anthropic-beta".into(), "oauth-2025-04-20".into()),
        ],
        body: None,
    };
    runtime
        .fetch
        .fetch(request, Some(AbortSignal::timeout(5000.0).ok()?))
        .await
        .ok()
}
async fn read_response(response: pi_ai::request::HttpResponse) -> Result<JsValue, UsageError> {
    let invalid = || UsageError::new("Invalid Claude usage response: expected JSON subscription windows");
    let text = pi_ai::request::read_body_text(response.body)
        .await
        .map_err(|_| invalid())?;
    js_json::parse_lossless(text).map_err(|_| invalid())
}
pub(super) async fn fetch_subscription(token: &JsString, runtime: &UsageRuntime) -> Result<JsValue, UsageError> {
    let deadline = (runtime.now)() + MAX_WAIT_MS;
    for attempt in 0..MAX_ATTEMPTS {
        let backoff = 250.0 * 2_f64.powi(attempt as i32) * (1.0 + (runtime.random)().map_err(UsageError::new)?);
        let response = fetch_response(token, runtime).await;
        if response.as_ref().is_some_and(|r| (200..300).contains(&r.status)) {
            return read_response(response.expect("checked response")).await;
        }
        let now = (runtime.now)();
        let delay = response
            .as_ref()
            .and_then(|r| {
                r.headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
            })
            .and_then(|(_, value)| retry_after_ms(value, now));
        let error = response
            .as_ref()
            .map(|r| http_error(r.status, delay, now))
            .unwrap_or_else(|| UsageError::new("Claude usage network request failed after bounded attempts"));
        let transient = response
            .as_ref()
            .is_none_or(|r| r.status == 429 || (500..600).contains(&r.status));
        let wait = backoff.max(delay.unwrap_or(0.0));
        if !transient || now + wait > deadline || attempt + 1 == MAX_ATTEMPTS {
            return Err(error);
        }
        (runtime.sleep)(wait).await;
    }
    Err(UsageError::new("Claude usage request failed"))
}
fn current_token(account: &UsageAccount, now: f64) -> Result<Option<JsString>, UsageError> {
    let credentials = account_profiles::read_usage_credentials(
        &account.home,
        account.profile.as_deref(),
        account.config_dir.as_deref(),
    )
    .map_err(|e| UsageError::new(e.message))?;
    let oauth = credentials
        .get("claudeAiOauth")
        .filter(|v| v.as_object().is_some())
        .ok_or_else(|| UsageError::new("Missing Claude OAuth access token for current account"))?;
    let token = oauth
        .get("accessToken")
        .and_then(JsValue::as_string)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| UsageError::new("Missing Claude OAuth access token for current account"))?;
    if let Some(expiry) = oauth.get("expiresAt") {
        let expiry = expiry
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| UsageError::new("Invalid Claude credentials: access token expiry"))?;
        if expiry <= now {
            return Ok(None);
        }
    }
    Ok(Some(token.clone()))
}
async fn refresh_token(account: &UsageAccount, bridge: &BridgeProvider) -> Result<bool, UsageError> {
    let executable = bridge.executable.clone().map_err(|_| {
        UsageError::new(format!(
            "Expired Claude OAuth access token for current account; no bundled Claude binary for {}-{}",
            if cfg!(target_os = "linux") { "linux" } else { "darwin" },
            if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
        ))
    })?;
    let config_dir = match &account.profile {
        Some(name) => {
            Some(account_profiles::prepare_profile(&account.home, name).map_err(|_| UsageError::unexpected())?)
        }
        None => account.config_dir.clone(),
    };
    let mut command = tokio::process::Command::new(executable);
    command
        .args(REFRESH_ARGS)
        .current_dir(std::env::temp_dir())
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null());
    if let Some(dir) = config_dir {
        command.env("CLAUDE_CONFIG_DIR", dir);
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), command.output()).await;
    Ok(matches!(result,Ok(Ok(output)) if output.status.success()))
}
async fn read_access_token(
    account: &UsageAccount,
    bridge: &BridgeProvider,
    runtime: &UsageRuntime,
) -> Result<JsString, UsageError> {
    if let Some(token) = current_token(account, (runtime.now)())? {
        return Ok(token);
    }
    let succeeded = refresh_token(account, bridge).await?;
    current_token(account, (runtime.now)())?.ok_or_else(|| {
        UsageError::new(format!(
            "Expired Claude OAuth access token for current account; claude CLI refresh {}",
            if succeeded { "did not renew it" } else { "failed" }
        ))
    })
}
fn window_percent(window: &JsValue) -> Result<f64, UsageError> {
    if window.as_object().is_none() {
        return Err(UsageError::new(
            "Invalid Claude usage response: expected subscription window",
        ));
    }
    window
        .get("utilization")
        .and_then(JsValue::as_f64)
        .filter(|p| p.is_finite() && (0.0..=100.0).contains(p))
        .ok_or_else(|| UsageError::new("Invalid Claude usage response: invalid percent"))
}
fn read_reset(reset: Option<&JsValue>) -> Result<JsValue, UsageError> {
    use std::sync::LazyLock;
    static ISO: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$")
            .expect("static ISO regex")
    });
    if matches!(reset, Some(JsValue::Null)) {
        return Ok(JsValue::Null);
    }
    let invalid = || UsageError::new("Invalid Claude usage response: invalid reset");
    let text = reset
        .and_then(JsValue::as_string)
        .ok_or_else(invalid)?
        .to_utf8()
        .map_err(|_| invalid())?;
    if !ISO.is_match(&text) {
        return Err(invalid());
    }
    let millis = pi_js_compat::date::date_parse(&text);
    let iso = pi_js_compat::date::to_iso_string_value(millis).map_err(|_| invalid())?;
    Ok(JsValue::String(iso.into()))
}
fn read_window(window: Option<&JsValue>) -> Result<JsValue, UsageError> {
    if matches!(window, Some(JsValue::Null)) {
        return Ok(JsValue::Null);
    }
    let window =
        window.ok_or_else(|| UsageError::new("Invalid Claude usage response: expected subscription window"))?;
    Ok(JsValue::object([
        ("percent", JsValue::Number(window_percent(window)?)),
        ("resetsAt", read_reset(window.get("resets_at"))?),
    ]))
}
fn render_subscription(name: &str, response: &JsValue) -> Result<JsString, UsageError> {
    if response.as_object().is_none() {
        return Err(UsageError::new(
            "Invalid Claude usage response: expected subscription windows",
        ));
    }
    let mut lines: Vec<JsString> = vec![
        format!("Claude account: {name}").into(),
        "Subscription quota (not session token usage)".into(),
    ];
    for (field, label, optional) in [
        ("five_hour", "5-hour", false),
        ("seven_day", "7-day", false),
        ("seven_day_opus", "7-day Opus", true),
        ("seven_day_sonnet", "7-day Sonnet", true),
    ] {
        let window = if optional && response.get(field).is_none() {
            JsValue::Null
        } else {
            read_window(response.get(field))?
        };
        let line = if matches!(window, JsValue::Null) {
            format!("{label}: unavailable").into()
        } else {
            let percent = window
                .get("percent")
                .and_then(JsValue::as_f64)
                .expect("validated percent");
            let reset = match window.get("resetsAt").and_then(JsValue::as_string) {
                Some(iso) => pi_js_compat::js_format!("resets {} (UTC)", iso),
                None => "reset unavailable".into(),
            };
            pi_js_compat::js_format!(
                "{}: {}% — {}",
                label,
                pi_js_compat::js_json::to_js_string(&JsValue::Number(percent)).map_err(UsageError::new)?,
                reset
            )
        };
        lines.push(line);
    }
    Ok(JsString::join(lines, "\n"))
}
#[derive(Clone)]
pub(super) struct UsageService {
    pub bridge: BridgeProvider,
    pub home: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub runtime: UsageRuntime,
}
impl UsageService {
    fn configured_names(&self) -> Result<Option<Vec<String>>, JsString> {
        if self
            .bridge
            .settings
            .get("accountProfiles")
            .is_some_and(|v| v.as_array().is_none())
        {
            return Err("Invalid Claude account profiles configuration".into());
        }
        account_profiles::configured_names(&self.bridge.settings)
            .map_err(|e| e.message)?
            .map(|names| {
                names
                    .iter()
                    .map(|s| s.to_utf8().map_err(|e| e.to_string().into()))
                    .collect()
            })
            .transpose()
    }
    fn account(&self, name: String, profile: bool) -> UsageAccount {
        UsageAccount {
            home: self.home.clone(),
            profile: profile.then(|| name.clone()),
            name,
            config_dir: if profile { None } else { self.config_dir.clone() },
        }
    }
    fn current_account(&self) -> Result<Option<UsageAccount>, JsString> {
        if self.configured_names()?.is_none() {
            let name = account_profiles::read_active_profile(&self.home, self.config_dir.as_deref())
                .map_err(|e| e.message)?
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "current".into());
            return Ok(Some(self.account(name, false)));
        }
        let mut accounts = self
            .bridge
            .accounts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let selected = accounts
            .as_mut()
            .map_err(|e| e.message.clone())?
            .as_mut()
            .map(|m| m.selected_profile())
            .transpose()
            .map_err(|e| e.message)?
            .flatten();
        Ok(selected.map(|name| self.account(name, true)))
    }
    async fn response(&self, account: &UsageAccount) -> Result<JsValue, UsageError> {
        let token = read_access_token(account, &self.bridge, &self.runtime).await?;
        fetch_subscription(&token, &self.runtime).await
    }
    pub async fn all(&self) -> Result<JsValue, JsString> {
        let names = self.configured_names()?;
        let current = self.current_account()?;
        let accounts = match names {
            Some(names) => names.into_iter().map(|name| self.account(name, true)).collect(),
            None => vec![
                current
                    .clone()
                    .ok_or_else(|| JsString::from("No selected Claude account profile"))?,
            ],
        };
        let mut results = Vec::new();
        for account in accounts {
            let mut row = vec![
                ("account", JsValue::String(account.name.as_str().into())),
                (
                    "selected",
                    JsValue::Bool(current.as_ref().is_some_and(|c| c.name == account.name)),
                ),
            ];
            let usage = async {
                let response = self.response(&account).await?;
                if response.as_object().is_none() {
                    return Err(UsageError::new(
                        "Invalid Claude usage response: expected subscription windows",
                    ));
                }
                Ok((
                    read_window(response.get("five_hour"))?,
                    read_window(response.get("seven_day"))?,
                ))
            }
            .await;
            match usage {
                Ok((five, weekly)) => row.extend([("fiveHour", five), ("weekly", weekly)]),
                Err(e) => row.push((
                    "error",
                    JsValue::String(e.report_message("Cannot read Claude account for subscription usage")),
                )),
            }
            results.push(JsValue::object(row));
        }
        Ok(JsValue::Array(results))
    }
    pub(super) async fn recheck(&self, model: &str, signal: &AbortSignal) -> Result<Vec<String>, JsString> {
        let names = self
            .configured_names()?
            .ok_or_else(|| JsString::from(super::account_state::ACCOUNTS_EXHAUSTED))?;
        let mut failures = Vec::new();
        let mut throttled = Vec::new();
        for name in names {
            assert_recheck_active(signal)?;
            let checked_at = (self.runtime.now)() / 1000.0;
            let result = self
                .response(&self.account(name.clone(), true))
                .await
                .and_then(|response| has_capacity(&response, model));
            assert_recheck_active(signal)?;
            match result {
                Ok(true) => {
                    let mut accounts = self
                        .bridge
                        .accounts
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(manager) = accounts.as_mut().map_err(|e| e.message.clone())?
                        && manager.recover(&name, checked_at).map_err(|e| e.message)?
                    {
                        return Ok(Vec::new());
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    if error.status == Some(429) {
                        throttled.push(name.clone());
                    }
                    failures.push(pi_js_compat::js_format!(
                        "{}: {}",
                        name,
                        error.report_message("Cannot read subscription usage")
                    ));
                }
            }
        }
        if !throttled.is_empty() {
            return Ok(throttled);
        }
        let detail = if failures.is_empty() {
            JsString::default()
        } else {
            JsString::from(" Live quota recheck failed — ") + JsString::join(failures, "; ")
        };
        Err(JsString::from(super::account_state::ACCOUNTS_EXHAUSTED) + detail)
    }

    async fn report(&self, args: Option<&JsString>, ctx: &ExtensionContext, pi: &ExtensionApi) -> Result<(), JsString> {
        if args.is_none_or(|a| !a.trim().is_empty()) {
            return ctx.ui().map_err(|e| e.caught_message())?.notify(&"Unsupported /usage arguments for Claude; use /usage without arguments. Claude quota reset is unsupported.".into(),Some(NotifyType::Warning)).map_err(|e| e.message);
        }
        let result = async {
            let account = self
                .current_account()
                .map_err(|_| UsageError::new("Cannot read Claude current account for subscription usage"))?
                .ok_or_else(|| {
                    UsageError::new("No selected Claude account profile available for subscription usage")
                })?;
            render_subscription(&account.name, &self.response(&account).await?)
        }
        .await;
        match result {
            Ok(content) => pi
                .send_message(
                    CustomMessagePayload {
                        custom_type: "claude-usage".into(),
                        content: CustomMessageContent::Text(content),
                        display: true,
                        details: None,
                    },
                    None,
                )
                .map_err(|e| e.caught_message()),
            Err(error) => ctx
                .ui()
                .map_err(|e| e.caught_message())?
                .notify(
                    &error.report_message("Cannot read Claude current account for subscription usage"),
                    Some(NotifyType::Error),
                )
                .map_err(|e| e.message),
        }
    }
}
fn assert_recheck_active(signal: &AbortSignal) -> Result<(), JsString> {
    if signal.aborted() {
        Err("Claude account quota recheck cancelled".into())
    } else {
        Ok(())
    }
}
fn has_capacity(response: &JsValue, model: &str) -> Result<bool, UsageError> {
    if response.as_object().is_none() {
        return Err(UsageError::new(
            "Invalid Claude usage response: expected subscription windows",
        ));
    }
    let required = |field| {
        response
            .get(field)
            .ok_or_else(|| UsageError::new("Invalid Claude usage response: expected subscription window"))
            .and_then(window_percent)
    };
    let mut percentages = vec![required("five_hour")?, required("seven_day")?];
    for (family, field) in [("opus", "seven_day_opus"), ("sonnet", "seven_day_sonnet")] {
        if model.contains(family)
            && let Some(window) = response.get(field).filter(|v| !matches!(v, JsValue::Null))
        {
            percentages.push(window_percent(window)?);
        }
    }
    Ok(percentages.iter().all(|p| *p < 100.0))
}

pub(super) fn register(pi: &ExtensionApi, bridge: BridgeProvider) -> Result<(), HandlerError> {
    let home = pi_js_compat::node_os::homedir().map_err(|e| HandlerError::new(e.to_string()))?;
    let service = UsageService {
        bridge,
        home: PathBuf::from(home.to_utf8().map_err(|e| HandlerError::new(e.to_string()))?),
        config_dir: std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from),
        runtime: UsageRuntime::default(),
    };
    let all_service = service.clone();
    let unsubscribe_all = pi
        .events
        .on_request(
            &"claude-bridge:usage-all-request".into(),
            Arc::new(move |request| {
                if request.response.is_none() {
                    let service = all_service.clone();
                    request.response = Some(Box::pin(async move { service.all().await }));
                }
                Ok(())
            }),
        )
        .map_err(HandlerError::new)?;
    let api = pi.clone();
    let unsubscribe = pi
        .events
        .on_request(
            &"claude-bridge:usage-request".into(),
            Arc::new(move |request| {
                if request.response.is_some() {
                    return Ok(());
                }
                let ctx = request
                    .context
                    .as_ref()
                    .and_then(|ctx| ctx.downcast_ref::<ExtensionCommandContext>())
                    .map(|c| c.context.clone())
                    .or_else(|| {
                        request
                            .context
                            .as_ref()
                            .and_then(|ctx| ctx.downcast_ref::<ExtensionContext>())
                            .cloned()
                    });
                let args = request.data.get("args").and_then(JsValue::as_string).cloned();
                let service = service.clone();
                let pi = api.clone();
                request.response = Some(Box::pin(async move {
                    let ctx = ctx.ok_or_else(|| JsString::from("Claude usage request context unavailable"))?;
                    service.report(args.as_ref(), &ctx, &pi).await?;
                    Ok(JsValue::Null)
                }));
                Ok(())
            }),
        )
        .map_err(HandlerError::new)?;
    pi.on_session_shutdown(
        move |_, _| -> crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>> {
            unsubscribe();
            unsubscribe_all();
            Box::pin(async { Ok(None) })
        },
    )
    .map_err(HandlerError::new)
}

#[cfg(test)]
mod tests;
