use super::{ControlDbError, with_control_db};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReminderStatus {
    Open,
    Done,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderListStatus {
    Open,
    Done,
    All,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub title: JsString,
    pub notes: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<JsString>,
    pub due_at: JsString,
    pub status: ReminderStatus,
    pub created_at: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<JsString>,
}
#[derive(Debug, Clone)]
pub struct CreateReminderInput {
    pub title: JsString,
    pub notes: Option<JsString>,
    pub project_path: Option<JsString>,
    pub due_at: JsString,
}
#[derive(Debug, Clone, Default)]
pub struct ReminderPatch {
    pub title: Option<JsString>,
    pub notes: Option<JsString>,
    pub project_path: Option<Option<JsString>>,
    pub due_at: Option<JsString>,
}

const COLUMNS: &str = "id, title, notes, project_path, due_at, status, created_at, completed_at, delivered_at";
fn from_row(row: &Row<'_>) -> rusqlite::Result<Reminder> {
    let status: String = row.get(5)?;
    let status = match status.as_str() {
        "open" => ReminderStatus::Open,
        "done" => ReminderStatus::Done,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Reminder {
        id: row.get(0)?,
        title: row.get(1)?,
        notes: row.get(2)?,
        project_path: row.get(3)?,
        due_at: row.get(4)?,
        status,
        created_at: row.get(6)?,
        completed_at: row.get(7)?,
        delivered_at: row.get(8)?,
    })
}
fn validate_id(id: i64) -> Result<(), ControlDbError> {
    if id <= 0 || id > 9_007_199_254_740_991 {
        return Err(ControlDbError::new("Reminder ID must be a positive safe integer"));
    }
    Ok(())
}
fn validate_title(title: &JsString) -> Result<(), ControlDbError> {
    if title.trim().units().is_empty() {
        return Err(ControlDbError::new("Reminder title must not be empty"));
    }
    Ok(())
}
fn canonicalize(value: &JsString) -> Result<JsString, ControlDbError> {
    let invalid = || ControlDbError::new("Reminder dueAt must be a valid ISO timestamp with an explicit offset");
    let value = encode_node_utf8(value);
    let re = regex::Regex::new(r"^(\d{4})-(\d{2})-(\d{2})T(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d(?:\.\d+)?)?(?:Z|[+-](?:[01]\d|2[0-3]):?[0-5]\d)$").expect("constant regex");
    let cap = re.captures(&value).ok_or_else(invalid)?;
    let midnight = format!("{}-{}-{}T00:00:00.000Z", &cap[1], &cap[2], &cap[3]);
    let ms = pi_js_compat::date::date_parse(&midnight);
    let fields = pi_js_compat::date::utc_date_fields(ms as i64).ok_or_else(invalid)?;
    if !ms.is_finite() || fields.day != cap[3].parse::<i64>().map_err(|_| invalid())? {
        return Err(invalid());
    }
    pi_js_compat::date::to_iso_string_value(pi_js_compat::date::date_parse(&value))
        .map(JsString::from)
        .map_err(|_| invalid())
}
fn now() -> JsString {
    pi_js_compat::date::to_iso_string_value(pi_js_compat::date::js_now_millis())
        .expect("system clock in JS date range")
        .into()
}
fn missing(id: i64) -> ControlDbError {
    ControlDbError::new(format!("Reminder {id} not found"))
}

pub fn create_reminder(path: impl Into<JsString>, input: &CreateReminderInput) -> Result<Reminder, ControlDbError> {
    validate_title(&input.title)?;
    let due = canonicalize(&input.due_at)?;
    with_control_db(&path.into(), |db| {
        Ok(db.query_row(&format!("INSERT INTO reminders (title, notes, project_path, due_at, status, created_at) VALUES (?, ?, ?, ?, 'open', ?) RETURNING {COLUMNS}"), params![input.title, input.notes.as_ref().unwrap_or(&JsString::from("")), input.project_path, due, now()], from_row)?)
    })
}
pub fn list_reminders(path: impl Into<JsString>, status: ReminderListStatus) -> Result<Vec<Reminder>, ControlDbError> {
    let status = match status {
        ReminderListStatus::Open => "open",
        ReminderListStatus::Done => "done",
        ReminderListStatus::All => "all",
    };
    with_control_db(&path.into(), |db| {
        let mut stmt = db.prepare(&format!(
            "SELECT {COLUMNS} FROM reminders WHERE (? = 'all' OR status = ?) ORDER BY due_at, id"
        ))?;
        Ok(stmt
            .query_map(params![status, status], from_row)?
            .collect::<Result<Vec<_>, _>>()?)
    })
}
pub fn update_reminder(path: impl Into<JsString>, id: i64, patch: &ReminderPatch) -> Result<Reminder, ControlDbError> {
    validate_id(id)?;
    if patch.title.is_none() && patch.notes.is_none() && patch.project_path.is_none() && patch.due_at.is_none() {
        return Err(ControlDbError::new("Reminder edit must contain at least one field"));
    }
    if let Some(title) = &patch.title {
        validate_title(title)?;
    }
    let due = patch.due_at.as_ref().map(canonicalize).transpose()?;
    with_control_db(&path.into(), |db| {
        db.query_row(&format!("UPDATE reminders SET title = COALESCE(?, title), notes = COALESCE(?, notes), project_path = CASE WHEN ? THEN ? ELSE project_path END, due_at = COALESCE(?, due_at), delivered_at = CASE WHEN COALESCE(?, due_at) = due_at THEN delivered_at ELSE NULL END WHERE id = ? RETURNING {COLUMNS}"), params![patch.title, patch.notes, patch.project_path.is_some(), patch.project_path.as_ref().and_then(|v| v.as_ref()), due, due, id], from_row).optional()?.ok_or_else(|| missing(id))
    })
}
pub fn complete_reminder(path: impl Into<JsString>, id: i64) -> Result<Reminder, ControlDbError> {
    validate_id(id)?;
    with_control_db(&path.into(), |db| {
        db.query_row(&format!("UPDATE reminders SET status = 'done', completed_at = COALESCE(completed_at, ?) WHERE id = ? RETURNING {COLUMNS}"), params![now(),id], from_row).optional()?.ok_or_else(|| missing(id))
    })
}
pub fn reopen_reminder(path: impl Into<JsString>, id: i64) -> Result<Reminder, ControlDbError> {
    validate_id(id)?;
    with_control_db(&path.into(), |db| {
        db.query_row(&format!("UPDATE reminders SET status = 'open', completed_at = NULL, delivered_at = NULL WHERE id = ? RETURNING {COLUMNS}"), [id], from_row).optional()?.ok_or_else(|| missing(id))
    })
}
pub fn list_undelivered_due_reminders(
    path: impl Into<JsString>,
    now: &JsString,
) -> Result<Vec<Reminder>, ControlDbError> {
    with_control_db(&path.into(), |db| {
        let mut stmt = db.prepare(&format!("SELECT {COLUMNS} FROM reminders WHERE status = 'open' AND delivered_at IS NULL AND due_at <= ? ORDER BY due_at, id"))?;
        Ok(stmt.query_map([now], from_row)?.collect::<Result<Vec<_>, _>>()?)
    })
}
pub fn mark_reminder_delivered(
    path: impl Into<JsString>,
    id: i64,
    due_at: &JsString,
    delivered_at: &JsString,
) -> Result<Option<Reminder>, ControlDbError> {
    validate_id(id)?;
    with_control_db(&path.into(), |db| {
        Ok(db.query_row(&format!("UPDATE reminders SET delivered_at = ? WHERE id = ? AND due_at = ? AND status = 'open' AND delivered_at IS NULL RETURNING {COLUMNS}"), params![delivered_at,id,due_at], from_row).optional()?)
    })
}
pub fn delete_reminder(path: impl Into<JsString>, id: i64) -> Result<(), ControlDbError> {
    validate_id(id)?;
    with_control_db(&path.into(), |db| {
        if db.execute("DELETE FROM reminders WHERE id = ?", [id])? != 1 {
            return Err(missing(id));
        }
        Ok(())
    })
}
