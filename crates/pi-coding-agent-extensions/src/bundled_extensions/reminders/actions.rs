use crate::core::session_control_db::{
    CreateReminderInput, Reminder, ReminderListStatus, ReminderPatch, ReminderStatus, complete_reminder,
    create_reminder, delete_reminder, list_reminders, reopen_reminder, update_reminder,
};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::reminder_time::{format_reminder_due_at, is_reminder_due_today, parse_reminder_due_at};

pub const REMINDER_ACTIONS: &[&str] = &["list", "add", "edit", "reschedule", "complete", "reopen", "delete"];
pub const REMINDER_LIST_STATUSES: &[&str] = &["open", "done", "all", "due"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReminderAction {
    #[default]
    List,
    Add,
    Edit,
    Reschedule,
    Complete,
    Reopen,
    Delete,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListStatus {
    #[default]
    Open,
    Done,
    All,
    Due,
}

#[derive(Debug, Clone, Default)]
pub struct ReminderRequest {
    pub action: ReminderAction,
    pub id: Option<i64>,
    pub title: Option<JsString>,
    pub notes: Option<JsString>,
    pub project_path: Option<Option<JsString>>,
    pub due_at: Option<JsString>,
    pub status: Option<ListStatus>,
}

fn require_text(value: Option<&JsString>, field: &str) -> Result<JsString, String> {
    let text = value.map(JsString::trim).unwrap_or_default();
    if text.is_empty() {
        return Err(format!("{field} is required."));
    }
    Ok(text)
}

fn parse_due(value: &JsString) -> Result<JsString, String> {
    parse_reminder_due_at(&encode_node_utf8(value)).map(JsString::from)
}

pub fn execute_reminder_action(path: &JsString, request: &ReminderRequest, now: f64) -> Result<Vec<Reminder>, String> {
    match request.action {
        ReminderAction::List => {
            let status = request.status.unwrap_or_default();
            let store_status = match status {
                ListStatus::Open | ListStatus::Due => ReminderListStatus::Open,
                ListStatus::Done => ReminderListStatus::Done,
                ListStatus::All => ReminderListStatus::All,
            };
            let reminders = list_reminders(path.clone(), store_status).map_err(|error| error.to_string())?;
            if status != ListStatus::Due {
                return Ok(reminders);
            }
            let mut due = Vec::new();
            for reminder in reminders {
                if is_reminder_due_today(&encode_node_utf8(&reminder.due_at), now)? {
                    due.push(reminder);
                }
            }
            Ok(due)
        }
        ReminderAction::Add => {
            let title = require_text(request.title.as_ref(), "title")?;
            let due_at = parse_due(&require_text(request.due_at.as_ref(), "dueAt")?)?;
            let input = CreateReminderInput {
                title,
                notes: request.notes.clone(),
                project_path: request.project_path.clone().flatten(),
                due_at,
            };
            create_reminder(path.clone(), &input)
                .map(|reminder| vec![reminder])
                .map_err(|error| error.to_string())
        }
        action => {
            let id = request.id.ok_or_else(|| "Reminder id is required.".to_owned())?;
            match action {
                ReminderAction::Delete => {
                    delete_reminder(path.clone(), id).map_err(|error| error.to_string())?;
                    Ok(Vec::new())
                }
                ReminderAction::Complete => complete_reminder(path.clone(), id)
                    .map(|r| vec![r])
                    .map_err(|e| e.to_string()),
                ReminderAction::Reopen => reopen_reminder(path.clone(), id)
                    .map(|r| vec![r])
                    .map_err(|e| e.to_string()),
                ReminderAction::Edit | ReminderAction::Reschedule => {
                    let patch = if action == ReminderAction::Reschedule {
                        ReminderPatch {
                            due_at: Some(parse_due(&require_text(request.due_at.as_ref(), "dueAt")?)?),
                            ..Default::default()
                        }
                    } else {
                        ReminderPatch {
                            title: request.title.clone(),
                            notes: request.notes.clone(),
                            project_path: request.project_path.clone(),
                            due_at: request.due_at.as_ref().map(parse_due).transpose()?,
                        }
                    };
                    update_reminder(path.clone(), id, &patch)
                        .map(|r| vec![r])
                        .map_err(|e| e.to_string())
                }
                ReminderAction::List | ReminderAction::Add => unreachable!(),
            }
        }
    }
}

pub fn format_reminder(reminder: &Reminder) -> Result<JsString, String> {
    let status = match reminder.status {
        ReminderStatus::Open => "open",
        ReminderStatus::Done => "done",
    };
    let due = format_reminder_due_at(&encode_node_utf8(&reminder.due_at))?;
    let mut result = JsString::from(format!("#{} [{status}] due {due}: ", reminder.id));
    result.push_str(reminder.title.clone());
    if let Some(project) = reminder.project_path.as_ref().filter(|project| !project.is_empty()) {
        result.push_str(" (project: ");
        result.push_str(project.clone());
        result.push_str(")");
    }
    if !reminder.notes.is_empty() {
        result.push_str("\n  ");
        result.push_str(reminder.notes.replace("\n", "\n  "));
    }
    Ok(result)
}

pub fn describe_result(request: &ReminderRequest, reminders: &[Reminder]) -> Result<JsString, String> {
    match request.action {
        ReminderAction::Delete => Ok(format!(
            "Deleted reminder #{}.",
            request.id.map_or_else(|| "undefined".to_owned(), |id| id.to_string())
        )
        .into()),
        ReminderAction::List if reminders.is_empty() => {
            let status = match request.status.unwrap_or_default() {
                ListStatus::Open => "open",
                ListStatus::Done => "done",
                ListStatus::All => "all",
                ListStatus::Due => "due",
            };
            Ok(format!("No {status} reminders.").into())
        }
        ReminderAction::List => {
            let lines = reminders.iter().map(format_reminder).collect::<Result<Vec<_>, _>>()?;
            Ok(JsString::join(lines, "\n"))
        }
        action => {
            let verb = match action {
                ReminderAction::Add => "Added",
                ReminderAction::Edit => "Edited",
                ReminderAction::Reschedule => "Rescheduled",
                ReminderAction::Complete => "Completed",
                ReminderAction::Reopen => "Reopened",
                ReminderAction::List | ReminderAction::Delete => unreachable!(),
            };
            let reminder = reminders
                .first()
                .ok_or_else(|| "Reminder result is missing.".to_owned())?;
            let mut result = JsString::from(format!("{verb} reminder "));
            result.push_str(format_reminder(reminder)?);
            Ok(result)
        }
    }
}
