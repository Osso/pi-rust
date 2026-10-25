//! Claude bridge requests and the frozen `/usage all` report.

use std::sync::{Arc, LazyLock};

use pi_js_compat::date::date_parse_utf16;
use pi_js_compat::js_regex::JsRegex;

use super::*;
use crate::core::event_bus::EventRequest;
use crate::core::extensions::types::{MessageDelivery, SendMessageOptions};

const CODEX_PROVIDERS: [&str; 3] = ["openai-codex", "openai-codex-gc", "openai-codex-team"];
const ALL_TITLE: &str = "Account usage (all accounts); Resets = available reset credits; - = not reported";

struct UsageCell {
    percent: f64,
    resets_at: Option<f64>,
}

struct AccountUsageRow {
    provider: &'static str,
    account: JsString,
    five_hour: Option<UsageCell>,
    weekly: Option<UsageCell>,
    reset_credits_available: Option<f64>,
    status: Option<JsString>,
}

impl AccountUsageRow {
    fn new(provider: &'static str, account: JsString) -> Self {
        Self {
            provider,
            account,
            five_hour: None,
            weekly: None,
            reset_credits_available: None,
            status: None,
        }
    }

    fn with_status(mut self, status: JsString) -> Self {
        self.status = Some(status);
        self
    }
}

fn claude_status_row(status: JsString) -> AccountUsageRow {
    AccountUsageRow::new("Claude", JsString::default()).with_status(status)
}

fn sanitize_usage_text(text: &JsString) -> JsString {
    static PATTERNS: LazyLock<[(JsRegex, &str); 4]> = LazyLock::new(|| {
        [
            (
                JsRegex::with_flags(r"Bearer\s+\S+", "gi").expect("Bearer pattern"),
                "Bearer [redacted]",
            ),
            (
                JsRegex::new(r"\beyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").expect("JWT pattern"),
                "[redacted]",
            ),
            (
                JsRegex::new(r"\b(?:sk-|sk-ant-)[A-Za-z0-9_-]+").expect("API key pattern"),
                "[redacted]",
            ),
            (JsRegex::new(r"[\x00-\x1f\x7f-\x9f]").expect("control pattern"), " "),
        ]
    });
    let sanitized = PATTERNS.iter().fold(text.clone(), |text, (pattern, replacement)| {
        pattern.replace_all(text, *replacement)
    });
    sanitized.slice(0, 300)
}

async fn read_claude_account_rows(pi: &ExtensionApi) -> Vec<AccountUsageRow> {
    let mut request = EventRequest::new(JsValue::object(std::iter::empty::<(&str, JsValue)>()));
    if let Err(error) = pi
        .events
        .emit_request(&"claude-bridge:usage-all-request".into(), &mut request)
    {
        return vec![claude_status_row(
            JsString::from("error: ") + sanitize_usage_text(&error),
        )];
    }
    let Some(accounts) = request.response else {
        return vec![claude_status_row("bridge not loaded".into())];
    };
    match accounts.await {
        Ok(JsValue::Array(accounts)) => accounts.iter().map(format_claude_account_row).collect(),
        Ok(_) => vec![claude_status_row(
            "error: malformed Claude account usage response".into(),
        )],
        Err(error) => vec![claude_status_row(
            JsString::from("error: ") + sanitize_usage_text(&error),
        )],
    }
}

fn format_claude_account_row(value: &JsValue) -> AccountUsageRow {
    let account = value.get("account").and_then(JsValue::as_string);
    let selected = value.get("selected").and_then(JsValue::as_bool);
    let label = account.map(sanitize_usage_text).unwrap_or_else(|| "unknown".into());
    let suffix = if selected == Some(true) { " *" } else { "" };
    let mut row = AccountUsageRow::new("Claude", label + suffix);
    if account.is_none_or(|account| account.trim().is_empty()) || selected.is_none() {
        return row.with_status("error: malformed Claude account usage entry".into());
    }
    if let Some(error) = value.get("error") {
        return match error.as_string() {
            Some(error) => row.with_status(JsString::from("error: ") + sanitize_usage_text(error)),
            None => row.with_status("error: malformed Claude account usage entry".into()),
        };
    }
    match (
        parse_claude_window(value.get("fiveHour")),
        parse_claude_window(value.get("weekly")),
    ) {
        (Ok(five_hour), Ok(weekly)) => {
            row.five_hour = five_hour;
            row.weekly = weekly;
            row
        }
        _ => row.with_status("error: malformed Claude account usage entry".into()),
    }
}

fn parse_claude_window(value: Option<&JsValue>) -> Result<Option<UsageCell>, ()> {
    let value = value.ok_or(())?;
    if value.is_null() {
        return Ok(None);
    }
    let percent = value.get("percent").and_then(JsValue::as_f64).ok_or(())?;
    if !percent.is_finite() || percent < 0.0 {
        return Err(());
    }
    let resets_at = match value.get("resetsAt") {
        Some(JsValue::Null) => None,
        Some(JsValue::String(text)) => Some(parse_utc_iso_timestamp(text)? / MILLISECONDS_PER_SECOND),
        _ => return Err(()),
    };
    Ok(Some(UsageCell { percent, resets_at }))
}

fn parse_utc_iso_timestamp(text: &JsString) -> Result<f64, ()> {
    static ISO: LazyLock<JsRegex> =
        LazyLock::new(|| JsRegex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$").expect("UTC ISO pattern"));
    if !ISO.is_match(text) {
        return Err(());
    }
    let millis = date_parse_utf16(text.units());
    if millis.is_finite() { Ok(millis) } else { Err(()) }
}

pub(super) async fn dispatch_claude_usage_request(
    args: &JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
) -> Result<(), HandlerError> {
    let mut request = EventRequest::new(JsValue::object([("args", JsValue::String(args.clone()))]));
    request.context = Some(Arc::new(ctx.clone()));
    pi.events
        .emit_request(&"claude-bridge:usage-request".into(), &mut request)
        .map_err(HandlerError::new)?;
    let Some(handled) = request.response else {
        ctx.ui()?.notify(
            &"Claude bridge usage handler unavailable.".into(),
            Some(NotifyType::Error),
        )?;
        return Ok(());
    };
    handled.await.map_err(HandlerError::new)?;
    Ok(())
}

fn codex_usage_cell(window: Option<&CodexRateLimitWindow>) -> Option<UsageCell> {
    window.map(|window| UsageCell {
        percent: window.used_percent,
        resets_at: window.resets_at,
    })
}

async fn read_codex_account_row(ctx: &ExtensionCommandContext, fetch: &dyn Fetch, provider: &str) -> AccountUsageRow {
    let selected = ctx
        .model()
        .ok()
        .flatten()
        .is_some_and(|model| model.provider == provider);
    let suffix = if selected { " *" } else { "" };
    let row = AccountUsageRow::new("Codex", JsString::from(provider) + suffix);
    match read_logged_in_codex_row(ctx, fetch, provider, suffix).await {
        Ok(Some(row)) => row,
        Ok(None) => row.with_status("not logged in".into()),
        Err(error) => row.with_status(JsString::from("error: ") + sanitize_usage_text(&error.message)),
    }
}

async fn read_logged_in_codex_row(
    ctx: &ExtensionCommandContext,
    fetch: &dyn Fetch,
    provider: &str,
    selected: &str,
) -> Result<Option<AccountUsageRow>, HandlerError> {
    let registry = ctx.model_registry()?;
    let credential = registry.auth_storage().get(provider);
    if credential.is_none() {
        return Ok(None);
    }
    let usage = read_codex_usage_for_provider(ctx, fetch, provider).await?;
    Ok(Some(format_codex_account_row(&usage, provider, selected)))
}

fn format_codex_account_label(usage: &CodexUsage, provider: &str, selected: &str) -> JsString {
    let identity = usage
        .email
        .as_ref()
        .or(usage.account_id.as_ref())
        .cloned()
        .unwrap_or_else(|| provider.into());
    let plan = usage
        .plan_type
        .as_ref()
        .filter(|plan| !plan.is_empty())
        .map(|plan| js_format!(" ({})", sanitize_usage_text(plan)))
        .unwrap_or_default();
    js_format!("{}{} [{}]{}", sanitize_usage_text(&identity), plan, provider, selected)
}

fn format_codex_account_row(usage: &CodexUsage, provider: &str, selected: &str) -> AccountUsageRow {
    let label = format_codex_account_label(usage, provider, selected);
    let mut row = AccountUsageRow::new("Codex", label);
    row.reset_credits_available = usage.reset_credits_available;
    let limit = usage.limits.iter().find(|limit| limit.limit_id == "codex");
    let windows = [
        limit.and_then(|limit| limit.primary.as_ref()),
        limit.and_then(|limit| limit.secondary.as_ref()),
    ];
    row.five_hour = codex_usage_cell(
        windows
            .iter()
            .copied()
            .flatten()
            .find(|window| window.window_minutes == Some(FIVE_HOUR_WINDOW_MINUTES)),
    );
    row.weekly = codex_usage_cell(
        windows
            .iter()
            .copied()
            .flatten()
            .find(|window| window.window_minutes == Some(WEEKLY_WINDOW_MINUTES)),
    );
    row
}

fn format_usage_percent(cell: Option<&UsageCell>) -> String {
    cell.map(|cell| format_percent(cell.percent))
        .unwrap_or_else(|| "-".to_owned())
}

fn format_usage_reset(cell: Option<&UsageCell>) -> String {
    let Some(seconds) = cell.and_then(|cell| cell.resets_at) else {
        return "-".to_owned();
    };
    let millis = seconds * MILLISECONDS_PER_SECOND;
    if !millis.is_finite() || millis.abs() > 8_640_000_000_000_000.0 {
        return "unknown".to_owned();
    }
    let zone = jiff::tz::TimeZone::get("America/Chicago").expect("Central IANA time zone");
    let Some(date) = pi_js_compat::date::local_date_fields_in(millis as i64, &zone) else {
        return "unknown".to_owned();
    };
    // Intl's numeric year is unpadded and uses the year-of-era for BCE dates.
    let display_year = if date.year <= 0 { 1 - date.year } else { date.year };
    format!(
        "{}-{:02}-{:02} {:02}:{:02}",
        display_year,
        date.month + 1,
        date.day,
        date.hours,
        date.minutes
    )
}

fn format_account_columns(cells: [JsString; 6], account_width: usize) -> JsString {
    let widths = [5, 5, 6, 8, account_width, 0];
    JsString::join(
        cells.into_iter().zip(widths).map(|(cell, width)| cell.pad_end(width)),
        "  ",
    )
    .trim_end()
}

fn format_account_row(row: &AccountUsageRow, account_width: usize) -> JsString {
    let details = row.status.clone().unwrap_or_else(|| {
        format!(
            "{} | {}",
            format_usage_reset(row.five_hour.as_ref()),
            format_usage_reset(row.weekly.as_ref())
        )
        .into()
    });
    let credits = row
        .reset_credits_available
        .map(js_number_to_string)
        .unwrap_or_else(|| "-".to_owned());
    format_account_columns(
        [
            format_usage_percent(row.five_hour.as_ref()).into(),
            format_usage_percent(row.weekly.as_ref()).into(),
            credits.into(),
            row.provider.into(),
            row.account.clone(),
            details,
        ],
        account_width,
    )
}

pub(super) async fn handle_usage_all(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    fetch: &dyn Fetch,
) -> Result<(), HandlerError> {
    let (mut rows, codex_rows) = futures_util::join!(
        read_claude_account_rows(pi),
        futures_util::future::join_all(
            CODEX_PROVIDERS
                .iter()
                .map(|provider| read_codex_account_row(ctx, fetch, provider))
        ),
    );
    rows.extend(codex_rows);
    let table = format_account_table(&rows);
    send_usage_all_report(ctx, pi, table)
}

fn format_account_table(rows: &[AccountUsageRow]) -> JsString {
    let width = rows
        .iter()
        .map(|row| row.account.len())
        .max()
        .unwrap_or(0)
        .max("Account".len());
    let header = [
        "5h",
        "Week",
        "Resets",
        "Provider",
        "Account",
        "Resets Central (5h | week)",
    ]
    .map(JsString::from);
    let lines = std::iter::once(format_account_columns(header, width))
        .chain(rows.iter().map(|row| format_account_row(row, width)));
    JsString::join(lines, "\n")
}

fn send_usage_all_report(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    table: JsString,
) -> Result<(), HandlerError> {
    let content = js_format!("{}\n\n```\n{}\n```", ALL_TITLE, table);
    send_usage_report(ctx, pi, "usage-all", content, js_format!("{}\n{}", ALL_TITLE, table))
}

pub(super) fn send_usage_report(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    custom_type: &str,
    content: JsString,
    notification: JsString,
) -> Result<(), HandlerError> {
    let options = if ctx.is_idle()? {
        None
    } else {
        ctx.ui()?.notify(&notification, Some(NotifyType::Info))?;
        Some(SendMessageOptions {
            deliver_as: Some(MessageDelivery::NextTurn),
            ..Default::default()
        })
    };
    pi.send_message(
        CustomMessagePayload {
            custom_type: custom_type.into(),
            content: CustomMessageContent::Text(content),
            display: true,
            details: None,
        },
        options,
    )
}
