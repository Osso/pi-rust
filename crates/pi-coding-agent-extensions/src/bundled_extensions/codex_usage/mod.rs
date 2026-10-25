//! Bundled `codex-usage` extension (`packages/coding-agent/extensions/codex-usage/src/index.ts`).

mod bridge;

use std::sync::Arc;

use pi_ai::request::{CallbackError, Fetch, HttpRequest, read_body_text};
use pi_js_compat::base64::buffer_from_base64;
use pi_js_compat::date::utc_date_fields;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject as Map, JsString, JsValue, JsValue as Value};
use pi_js_compat::number::{js_number_to_string, to_fixed};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::extensions::types::{
    CustomMessageContent, CustomMessagePayload, ExtensionApi, ExtensionCommandContext, HandlerError, NotifyType,
};

const DEFAULT_CODEX_PROVIDER_ID: &str = "openai-codex";
const CODEX_API_ID: &str = "openai-codex-responses";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_RESET_CREDIT_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";
const ACCOUNT_ID_CLAIM: &str = "https://api.openai.com/auth";
const SECONDS_PER_MINUTE: f64 = 60.0;
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
const FIVE_HOUR_WINDOW_MINUTES: f64 = 300.0;
const WEEKLY_WINDOW_MINUTES: f64 = 10_080.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRateLimitWindow {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub used_percent: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub window_minutes: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub resets_at: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexCredits {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<JsString>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRateLimit {
    pub limit_id: JsString,
    pub limit_name: Option<JsString>,
    pub primary: Option<CodexRateLimitWindow>,
    pub secondary: Option<CodexRateLimitWindow>,
    pub credits: Option<CodexCredits>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsage {
    pub plan_type: Option<JsString>,
    pub account_id: Option<JsString>,
    pub email: Option<JsString>,
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub reset_credits_available: Option<f64>,
    pub limits: Vec<CodexRateLimit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountInfo {
    pub account_id: JsString,
    pub email: Option<JsString>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConsumeResetCreditResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub windows_reset: Option<f64>,
}

#[derive(Clone, Default)]
pub struct CodexUsageOptions {
    pub fetch: Option<Arc<dyn Fetch>>,
}

impl CodexUsageOptions {
    fn fetch(&self) -> Arc<dyn Fetch> {
        self.fetch.clone().unwrap_or_else(pi_ai::http::default_fetch)
    }
}

fn nullable_string(value: Option<&Value>) -> Option<JsString> {
    value.and_then(Value::as_string).cloned()
}

fn truthy_string(value: Option<&Value>) -> Option<JsString> {
    nullable_string(value).filter(|text| !text.is_empty())
}

fn nullable_number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

/// `JSON.parse(Buffer.from(part, "base64url").toString("utf8"))`: claims keep
/// exact UTF-16 (JSON escapes may hold unpaired surrogates).
fn decode_codex_token_payload(token: &JsString) -> Result<JsValue, JsString> {
    let parts: Vec<JsString> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid OpenAI Codex access token".into());
    }
    let bytes = buffer_from_base64(&parts[1]);
    let text = String::from_utf8_lossy(&bytes);
    js_json::parse_lossless(text.as_ref()).map_err(|error| error.to_string().into())
}

fn truthy_claim(value: Option<&JsValue>) -> Option<JsString> {
    value
        .and_then(JsValue::as_string)
        .filter(|text| !text.is_empty())
        .cloned()
}

fn resolve_email_claim(payload: &JsValue) -> Option<JsString> {
    if let Some(email) = truthy_claim(payload.get("email")) {
        return Some(email);
    }
    let profile_email = payload.get("profile").and_then(|profile| profile.get("email"));
    truthy_claim(profile_email)
}

pub fn resolve_codex_account_info(token: &JsString) -> Result<CodexAccountInfo, JsString> {
    let payload = decode_codex_token_payload(token)?;
    let account_id = payload
        .get(ACCOUNT_ID_CLAIM)
        .and_then(|claim| claim.get("chatgpt_account_id"));
    let Some(account_id) = truthy_claim(account_id) else {
        return Err("OpenAI Codex access token does not include a ChatGPT account id".into());
    };
    Ok(CodexAccountInfo {
        account_id,
        email: resolve_email_claim(&payload),
    })
}

pub fn resolve_codex_account_id(token: &JsString) -> Result<JsString, JsString> {
    Ok(resolve_codex_account_info(token)?.account_id)
}

pub fn resolve_codex_display_email(
    token: &JsString,
    stored_email: Option<&JsValue>,
) -> Result<Option<JsString>, JsString> {
    if let Some(email) = truthy_claim(stored_email) {
        return Ok(Some(email));
    }
    Ok(resolve_codex_account_info(token)?.email)
}

fn map_window(window: Option<&Value>) -> Option<CodexRateLimitWindow> {
    let window = window?;
    if window.is_null() {
        return None;
    }
    let used_percent = nullable_number(window.get("used_percent")).filter(|value| value.is_finite())?;
    let window_seconds = nullable_number(window.get("limit_window_seconds"));
    let window_minutes = window_seconds
        .filter(|seconds| *seconds > 0.0)
        .map(|seconds| (seconds / SECONDS_PER_MINUTE).ceil());
    Some(CodexRateLimitWindow {
        used_percent,
        window_minutes,
        resets_at: nullable_number(window.get("reset_at")),
    })
}

fn map_credits(credits: Option<&Value>) -> Option<CodexCredits> {
    let credits = credits?;
    if credits.is_null() {
        return None;
    }
    Some(CodexCredits {
        has_credits: credits.get("has_credits").and_then(Value::as_bool).unwrap_or(false),
        unlimited: credits.get("unlimited").and_then(Value::as_bool).unwrap_or(false),
        balance: nullable_string(credits.get("balance")),
    })
}

fn map_codex_rate_limit(
    limit_id: JsString,
    limit_name: Option<JsString>,
    rate_limit: Option<&Value>,
    credits: Option<&Value>,
) -> CodexRateLimit {
    let rate_limit = rate_limit.filter(|value| !value.is_null());
    CodexRateLimit {
        limit_id,
        limit_name,
        primary: map_window(rate_limit.and_then(|value| value.get("primary_window"))),
        secondary: map_window(rate_limit.and_then(|value| value.get("secondary_window"))),
        credits: map_credits(credits),
    }
}

fn map_additional_rate_limits(payloads: Option<&Value>) -> Vec<CodexRateLimit> {
    payloads
        .and_then(Value::as_array)
        .map(|payloads| {
            payloads
                .iter()
                .map(|payload| {
                    let metered_feature = truthy_string(payload.get("metered_feature"));
                    let limit_name_value = nullable_string(payload.get("limit_name"));
                    let limit_id = metered_feature
                        .clone()
                        .or_else(|| limit_name_value.clone().filter(|name| !name.is_empty()))
                        .unwrap_or_else(|| "codex_other".into());
                    map_codex_rate_limit(
                        limit_id,
                        limit_name_value.or_else(|| nullable_string(payload.get("metered_feature"))),
                        payload.get("rate_limit"),
                        None,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn map_codex_usage_payload(payload: &Value) -> CodexUsage {
    let mut limits = vec![map_codex_rate_limit(
        "codex".into(),
        None,
        payload.get("rate_limit"),
        payload.get("credits"),
    )];
    limits.extend(map_additional_rate_limits(payload.get("additional_rate_limits")));
    CodexUsage {
        plan_type: nullable_string(payload.get("plan_type")),
        account_id: None,
        email: None,
        reset_credits_available: payload
            .get("rate_limit_reset_credits")
            .and_then(|credits| credits.get("available_count"))
            .and_then(Value::as_f64),
        limits,
    }
}

fn format_limit(limit: &CodexRateLimit) -> JsString {
    let label = match limit.limit_name.as_ref().filter(|name| !name.is_empty()) {
        Some(limit_name) => js_format!("{} ({})", limit.limit_id, limit_name),
        None => limit.limit_id.clone(),
    };
    let details: Vec<JsString> = format_limit_details(limit)
        .into_iter()
        .map(|detail| js_format!("{}: {}", label, detail))
        .collect();
    if details.is_empty() {
        js_format!("{}: no limit data", label)
    } else {
        JsString::join(details, "\n")
    }
}

fn format_limit_details(limit: &CodexRateLimit) -> Vec<JsString> {
    let mut details = Vec::new();
    if let Some(primary) = &limit.primary {
        details.push(js_format!(
            "{} {}",
            format_window_label(primary, "primary"),
            format_window(primary)
        ));
    }
    if let Some(secondary) = &limit.secondary {
        details.push(js_format!(
            "{} {}",
            format_window_label(secondary, "secondary"),
            format_window(secondary)
        ));
    }
    if let Some(credits) = &limit.credits {
        details.push(format_credits(credits));
    }
    details
}

fn format_window_label(window: &CodexRateLimitWindow, fallback: &str) -> String {
    if window.window_minutes == Some(FIVE_HOUR_WINDOW_MINUTES) {
        return "5-hour usage".to_owned();
    }
    if window.window_minutes == Some(WEEKLY_WINDOW_MINUTES) {
        return "weekly usage".to_owned();
    }
    fallback.to_owned()
}

fn format_window(window: &CodexRateLimitWindow) -> String {
    let window_text = match window.window_minutes {
        None => "window".to_owned(),
        Some(minutes) => format!("{}m window", js_number_to_string(minutes)),
    };
    let reset_text = match window.resets_at {
        None => "unknown".to_owned(),
        Some(timestamp) => format_reset_time(timestamp),
    };
    format!(
        "{} of {window_text} resets {reset_text}",
        format_percent(window.used_percent)
    )
}

fn format_percent(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}%", js_number_to_string(value))
    } else {
        format!("{}%", to_fixed(value, 1))
    }
}

fn format_reset_time(timestamp_seconds: f64) -> String {
    let millis = timestamp_seconds * MILLISECONDS_PER_SECOND;
    if !millis.is_finite() {
        return "unknown".to_owned();
    }
    let Some(fields) = utc_date_fields(millis as i64) else {
        return "unknown".to_owned();
    };
    format!(
        "{}-{}-{} {}:{}",
        fields.year,
        pad(fields.month + 1),
        pad(fields.day),
        pad(fields.hours),
        pad(fields.minutes)
    )
}

fn pad(value: i64) -> String {
    format!("{value:02}")
}

fn format_credits(credits: &CodexCredits) -> JsString {
    if credits.unlimited {
        return "credits unlimited".into();
    }
    if let Some(balance) = credits.balance.as_ref().filter(|balance| !balance.is_empty()) {
        return js_format!("credits balance {}", balance);
    }
    if credits.has_credits {
        "credits available".into()
    } else {
        "credits unavailable".into()
    }
}

pub fn format_codex_usage(usage: &CodexUsage) -> JsString {
    let email_line = usage
        .email
        .as_ref()
        .filter(|email| !email.is_empty())
        .map(|email| js_format!("User: {}", email));
    let account_id_line = usage
        .account_id
        .as_ref()
        .filter(|account_id| !account_id.is_empty())
        .map(|account_id| js_format!("Account: {}", account_id));
    let mut lines: Vec<JsString> = vec!["OpenAI Codex usage".into()];
    if let Some(line) = email_line {
        lines.push(line);
    }
    if let Some(line) = account_id_line {
        lines.push(line);
    }
    lines.push(js_format!(
        "Plan: {}",
        usage.plan_type.clone().unwrap_or_else(|| "unknown".into())
    ));
    lines.push(
        format!(
            "Reset credits: {} available",
            js_number_to_string(usage.reset_credits_available.unwrap_or(0.0))
        )
        .into(),
    );
    lines.extend(usage.limits.iter().map(format_limit));
    JsString::join(lines, "\n")
}

pub fn format_reset_response(response: &ConsumeResetCreditResponse) -> String {
    match response.code.as_deref() {
        Some("reset") => format!(
            "Consumed reset credit. Windows reset: {}",
            js_number_to_string(response.windows_reset.unwrap_or(0.0))
        ),
        Some("nothing_to_reset") => "No active Codex rate-limit window needed a reset.".to_owned(),
        Some("no_credit") => "No reset credit available.".to_owned(),
        Some("already_redeemed") => "Reset credit was already redeemed.".to_owned(),
        _ => "Reset credit response received.".to_owned(),
    }
}

fn send_codex_usage_message(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    content: JsString,
) -> Result<(), HandlerError> {
    bridge::send_usage_report(ctx, pi, "codex-usage", content.clone(), content)
}

fn resolve_codex_provider_id(ctx: &ExtensionCommandContext) -> Result<String, HandlerError> {
    if let Some(model) = ctx.model()?
        && model.api == CODEX_API_ID
    {
        return Ok(model.provider);
    }
    Ok(DEFAULT_CODEX_PROVIDER_ID.to_owned())
}

async fn read_codex_access_token(ctx: &ExtensionCommandContext) -> Result<JsString, HandlerError> {
    let provider_id = resolve_codex_provider_id(ctx)?;
    read_codex_access_token_for_provider(ctx, &provider_id).await
}

async fn read_codex_access_token_for_provider(
    ctx: &ExtensionCommandContext,
    provider_id: &str,
) -> Result<JsString, HandlerError> {
    let token = ctx
        .model_registry()?
        .auth_storage()
        .get_api_key(
            provider_id,
            crate::core::auth_storage::GetApiKeyOptions {
                include_fallback: false,
                ..Default::default()
            },
        )
        .await
        .map_err(|error| HandlerError::new(error.message()))?;
    let Some(token) = token.filter(|token| !token.is_empty()) else {
        return Err(HandlerError::new(format!(
            "OpenAI Codex provider {provider_id} is not logged in. Run /login {provider_id}."
        )));
    };
    Ok(token)
}

/// `new Headers(...)` + `headers.set`: each value is converted to a
/// ByteString when set.
fn create_codex_headers(token: &JsString, extra: Vec<(String, String)>) -> Result<Vec<(String, String)>, HandlerError> {
    let mut headers = extra;
    let mut set = |name: &str, value: &JsString| -> Result<(), HandlerError> {
        let value = pi_ai::request::header_value(value).map_err(callback_error)?;
        if let Some(existing) = headers.iter_mut().find(|(key, _)| key.eq_ignore_ascii_case(name)) {
            existing.1 = value;
        } else {
            headers.push((name.to_owned(), value));
        }
        Ok(())
    };
    set("authorization", &(JsString::from("Bearer ") + token))?;
    set(
        "chatgpt-account-id",
        &resolve_codex_account_id(token).map_err(HandlerError::new)?,
    )?;
    set("originator", &"pi".into())?;
    set("user-agent", &"pi".into())?;
    Ok(headers)
}

async fn read_json_response(response: pi_ai::request::HttpResponse) -> Result<Value, HandlerError> {
    let status = response.status;
    let status_text = response.status_text.clone();
    let text = read_body_text(response.body)
        .await
        .map_err(|error| HandlerError::new(error.message))?;
    if !(200..300).contains(&status) {
        let detail = if text.is_empty() { status_text } else { text };
        return Err(HandlerError::new(format!(
            "OpenAI Codex usage request failed ({status}): {detail}"
        )));
    }
    if text.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    js_json::parse_lossless(&text).map_err(|error| HandlerError::new(error.into_message()))
}

async fn read_codex_usage(ctx: &ExtensionCommandContext, fetch: &dyn Fetch) -> Result<CodexUsage, HandlerError> {
    let provider_id = resolve_codex_provider_id(ctx)?;
    read_codex_usage_for_provider(ctx, fetch, &provider_id).await
}

async fn read_codex_usage_for_provider(
    ctx: &ExtensionCommandContext,
    fetch: &dyn Fetch,
    provider_id: &str,
) -> Result<CodexUsage, HandlerError> {
    let token = read_codex_access_token_for_provider(ctx, provider_id).await?;
    read_codex_usage_with_token(ctx, fetch, provider_id, &token)
        .await
        .map_err(|error| HandlerError::new(error.message.replace(token, "[redacted]")))
}

async fn read_stored_codex_email(
    ctx: &ExtensionCommandContext,
    provider_id: &str,
) -> Result<Option<JsString>, HandlerError> {
    let stored = ctx.model_registry()?.auth_storage().get(provider_id);
    Ok(stored.and_then(|credential| truthy_claim(credential.get("email"))))
}

async fn fetch_codex_usage_payload(fetch: &dyn Fetch, token: &JsString) -> Result<Value, HandlerError> {
    let response = fetch
        .fetch(
            HttpRequest {
                method: "GET".to_owned(),
                url: CODEX_USAGE_URL.to_owned(),
                headers: create_codex_headers(token, Vec::new())?,
                body: None,
            },
            None,
        )
        .await
        .map_err(callback_error)?;
    read_json_response(response).await
}

async fn read_codex_usage_with_token(
    ctx: &ExtensionCommandContext,
    fetch: &dyn Fetch,
    provider_id: &str,
    token: &JsString,
) -> Result<CodexUsage, HandlerError> {
    let account_info = resolve_codex_account_info(token).map_err(HandlerError::new)?;
    let stored_email = read_stored_codex_email(ctx, provider_id).await?;
    let payload = fetch_codex_usage_payload(fetch, token).await?;
    let mut usage = map_codex_usage_payload(&payload);
    usage.account_id = Some(account_info.account_id);
    usage.email = stored_email.or(account_info.email);
    Ok(usage)
}

async fn consume_codex_usage_reset_credit(
    ctx: &ExtensionCommandContext,
    fetch: &dyn Fetch,
) -> Result<ConsumeResetCreditResponse, HandlerError> {
    let token = read_codex_access_token(ctx).await?;
    let body = serde_json::to_vec(&json!({
        "redeem_request_id": pi_ai::utils::random_uuid::random_uuid()?,
    }))
    .expect("reset request body serializes");
    let response = fetch
        .fetch(
            HttpRequest {
                method: "POST".to_owned(),
                url: CODEX_RESET_CREDIT_URL.to_owned(),
                headers: create_codex_headers(
                    &token,
                    vec![("content-type".to_owned(), "application/json".to_owned())],
                )?,
                body: Some(body),
            },
            None,
        )
        .await
        .map_err(callback_error)?;
    let value = read_json_response(response).await?;
    js_json::from_value(&value).map_err(|error| HandlerError::new(error.to_string()))
}

fn callback_error(error: CallbackError) -> HandlerError {
    HandlerError::new(error.message)
}

async fn read_and_send_codex_usage(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    fetch: &dyn Fetch,
) -> Result<(), HandlerError> {
    let usage = read_codex_usage(ctx, fetch).await?;
    send_codex_usage_message(ctx, pi, format_codex_usage(&usage))
}

async fn reset_codex_usage_and_send_result(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    fetch: &dyn Fetch,
) -> Result<(), HandlerError> {
    let usage_before_reset = read_codex_usage(ctx, fetch).await?;
    if usage_before_reset.reset_credits_available.unwrap_or(0.0) <= 0.0 {
        send_codex_usage_message(
            ctx,
            pi,
            format_codex_usage(&usage_before_reset) + "\n\nNo reset credits available.",
        )?;
        return Ok(());
    }

    let reset_confirmed = ctx
        .ui()?
        .confirm(
            &"Are you sure you want to reset OpenAI Codex usage?".into(),
            &pi_js_compat::js_format!(
                "This will consume 1 of {} available reset credits.",
                js_number_to_string(usage_before_reset.reset_credits_available.unwrap_or(0.0))
            ),
            None,
        )
        .await
        .map_err(HandlerError::from)?;
    if !reset_confirmed {
        return Ok(());
    }

    let reset_response = consume_codex_usage_reset_credit(ctx, fetch).await?;
    let usage_after_reset = read_codex_usage(ctx, fetch).await?;
    let message =
        JsString::from(format_reset_response(&reset_response)) + "\n\n" + format_codex_usage(&usage_after_reset);
    send_codex_usage_message(ctx, pi, message)
}

async fn handle_usage_command(
    args: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    fetch: &dyn Fetch,
) -> Result<(), HandlerError> {
    let command_args = args.trim();
    if command_args == "all" {
        return bridge::handle_usage_all(ctx, pi, fetch).await;
    }
    if ctx.model()?.is_some_and(|model| model.provider == "claude-bridge") {
        return bridge::dispatch_claude_usage_request(args, ctx, pi).await;
    }
    if command_args == "reset" {
        return reset_codex_usage_and_send_result(ctx, pi, fetch).await;
    }
    if !command_args.is_empty() {
        ctx.ui()?
            .notify(&"Usage: /usage [all|reset]".into(), Some(NotifyType::Warning))?;
        return Ok(());
    }
    read_and_send_codex_usage(ctx, pi, fetch).await
}

fn argument_completions(
    prefix: &pi_js_compat::js_json::JsString,
) -> Result<Option<Vec<pi_js_compat::js_json::JsValue>>, pi_js_compat::js_json::JsString> {
    let matches: Vec<pi_js_compat::js_json::JsValue> = ["all", "reset"]
        .into_iter()
        .filter(|option| pi_js_compat::js_json::JsString::from(*option).starts_with(prefix))
        .map(|value| pi_js_compat::js_json!({"value": value, "label": value}))
        .collect();
    Ok((!matches.is_empty()).then_some(matches))
}

/// Bundled `codex-usage` extension.
pub fn codex_usage_extension(pi: &ExtensionApi, options: &CodexUsageOptions) -> Result<(), HandlerError> {
    let fetch = options.fetch();
    let command_pi = pi.clone();
    pi.register_command(
        "usage",
        Some("Show Codex or Claude usage, all accounts, or reset Codex usage".into()),
        Some(Arc::new(argument_completions)),
        Arc::new(move |args, ctx| {
            let pi = command_pi.clone();
            let fetch = Arc::clone(&fetch);
            Box::pin(async move { handle_usage_command(&args, &ctx, &pi, fetch.as_ref()).await })
        }),
    )
    .map_err(HandlerError::new)
}

/// Registers the extension factory under `<first-party:codex-usage>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry, fetch: Option<Arc<dyn Fetch>>) {
    registry.register(
        "<first-party:codex-usage>",
        Arc::new(move |pi| {
            let options = CodexUsageOptions { fetch: fetch.clone() };
            let result = codex_usage_extension(pi, &options);
            Box::pin(async move { result })
        }),
    );
}
