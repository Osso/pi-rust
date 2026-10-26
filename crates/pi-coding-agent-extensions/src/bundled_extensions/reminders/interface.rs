use super::actions::{
    ListStatus, REMINDER_ACTIONS, REMINDER_LIST_STATUSES, ReminderAction, ReminderRequest, describe_result,
    execute_reminder_action,
};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::string::is_js_whitespace_unit;
use serde_json::{Value, json};

pub const TOOL_DESCRIPTION: &str = "Persistent reminders shared by all Pi sessions and projects of this OS user: durable future follow-ups with an exact due time. Not a task list: active-goal steps, subtasks, test checklists and progress belong in PLAN.md. A reminder is context, not authorization: listing or reading one never starts or completes work.";
pub const TOOL_SNIPPET: &str =
    "List, add, edit, reschedule, complete, reopen, or delete persistent future follow-up reminders.";
pub const TOOL_GUIDELINES: &[&str] = &[
    "Use reminders for durable future follow-ups (for example 'recheck security advisories tomorrow at 09:00'); keep active work steps in PLAN.md.",
    "You may add a useful reminder without being asked; tell the user the reminder ID and due time you added.",
    "Reminders are context, not authorization: do not start, skip, or complete work because a reminder exists.",
    "Pass exact due times; resolve relative dates from the current date and ask the user when a date or time is ambiguous.",
];
pub const COMMAND_DESCRIPTION: &str = "List and manage persistent reminders across projects.";
pub const DUE_AT_HELP: &str = "Exact due time: local 'YYYY-MM-DD HH:mm' or 'YYYY-MM-DDTHH:mm', or ISO with Z/+HH:MM. Resolve relative dates yourself and ask the user when the date or time is ambiguous.";
pub const COMMAND_USAGE: &str = "Usage: /reminders [list [open|done|all|due]] | add <due> <title> | edit <id> <title> | notes <id> <text> | reschedule <id> <due> | done <id> | reopen <id> | delete <id>. <due> is 'YYYY-MM-DD HH:mm' local time or ISO with offset.";

pub fn reminder_tool_schema() -> Value {
    let literals = |values: &[&str]| {
        values
            .iter()
            .map(|v| json!({"const": v, "type": "string"}))
            .collect::<Vec<_>>()
    };
    json!({"type":"object", "properties": {
        "action": {"anyOf": literals(REMINDER_ACTIONS)},
        "id": {"type":"integer", "description":"Reminder ID for edit, reschedule, complete, reopen, delete."},
        "title": {"type":"string", "description":"Short reminder title. Required for add."},
        "notes": {"type":"string", "description":"Optional details."},
        "projectPath": {"type":"string", "description":"Optional project path as context; does not filter."},
        "dueAt": {"type":"string", "description":DUE_AT_HELP},
        "status": {"anyOf": literals(REMINDER_LIST_STATUSES), "description":"List filter: open (default), done, all, or due (overdue and due today)."}
    }, "required":["action"]})
}

fn parse_status(value: &JsString) -> Option<ListStatus> {
    if value == "open" {
        Some(ListStatus::Open)
    } else if value == "done" {
        Some(ListStatus::Done)
    } else if value == "all" {
        Some(ListStatus::All)
    } else if value == "due" {
        Some(ListStatus::Due)
    } else {
        None
    }
}

fn parse_id(value: Option<&JsString>) -> Result<i64, String> {
    let text = String::from_utf16(value.ok_or(COMMAND_USAGE)?.units()).map_err(|_| COMMAND_USAGE)?;
    let radix = if text.starts_with("0x") || text.starts_with("0X") {
        Some(16)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        Some(2)
    } else if text.starts_with("0o") || text.starts_with("0O") {
        Some(8)
    } else {
        None
    };
    let number = match radix {
        Some(radix) => u64::from_str_radix(&text[2..], radix).ok().map(|n| n as f64),
        None => text.parse::<f64>().ok(),
    };
    match number {
        Some(n) if n > 0.0 && n <= 9_007_199_254_740_991.0 && n.fract() == 0.0 => Ok(n as i64),
        _ => Err(COMMAND_USAGE.into()),
    }
}

fn local_date(word: &JsString) -> bool {
    let u = word.units();
    u.len() == 10
        && u[4] == 45
        && u[7] == 45
        && u.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || (48..=57).contains(c))
}
fn local_clock(word: &JsString) -> bool {
    let u = word.units();
    (u.len() == 5 || u.len() == 8)
        && u[2] == 58
        && (u.len() == 5 || u[5] == 58)
        && u.iter()
            .enumerate()
            .all(|(i, c)| i == 2 || (u.len() == 8 && i == 5) || (48..=57).contains(c))
}
fn take_due(words: &[JsString]) -> (JsString, usize) {
    if words.len() >= 2 && local_date(&words[0]) && local_clock(&words[1]) {
        (JsString::join(words[..2].iter().cloned(), " "), 2)
    } else {
        (
            words.first().cloned().unwrap_or_default(),
            usize::from(!words.is_empty()),
        )
    }
}

pub fn parse_reminders_command(args: &JsString) -> Result<ReminderRequest, String> {
    let words: Vec<_> = args
        .units()
        .split(|c| is_js_whitespace_unit(*c))
        .filter(|w| !w.is_empty())
        .map(|w| JsString::from_utf16(w.to_vec()))
        .collect();
    let verb = words.first().cloned().unwrap_or_else(|| "list".into());
    let rest = words.get(1..).unwrap_or_default();
    let mut request = ReminderRequest::default();
    if verb == "list" {
        request.status = Some(match rest.first() {
            None => ListStatus::Open,
            Some(status) => parse_status(status).ok_or(COMMAND_USAGE)?,
        });
    } else if verb == "add" {
        let (due, count) = take_due(rest);
        request.action = ReminderAction::Add;
        request.due_at = Some(due);
        request.title = Some(JsString::join(rest[count..].iter().cloned(), " "));
    } else if verb == "edit" || verb == "notes" {
        request.action = ReminderAction::Edit;
        request.id = Some(parse_id(rest.first())?);
        let text = JsString::join(rest.get(1..).unwrap_or_default().iter().cloned(), " ");
        if verb == "edit" {
            request.title = Some(text);
        } else {
            request.notes = Some(text);
        }
    } else if verb == "reschedule" {
        request.action = ReminderAction::Reschedule;
        request.id = Some(parse_id(rest.first())?);
        request.due_at = Some(take_due(rest.get(1..).unwrap_or_default()).0);
    } else {
        request.action = if verb == "done" {
            ReminderAction::Complete
        } else if verb == "reopen" {
            ReminderAction::Reopen
        } else if verb == "delete" {
            ReminderAction::Delete
        } else {
            return Err(COMMAND_USAGE.into());
        };
        request.id = Some(parse_id(rest.first())?);
    }
    Ok(request)
}

pub fn request_from_tool(params: &JsValue) -> Result<ReminderRequest, String> {
    let string = |key| params.get(key).and_then(JsValue::as_string).cloned();
    let action = match string("action").as_ref() {
        Some(v) if v == "list" => ReminderAction::List,
        Some(v) if v == "add" => ReminderAction::Add,
        Some(v) if v == "edit" => ReminderAction::Edit,
        Some(v) if v == "reschedule" => ReminderAction::Reschedule,
        Some(v) if v == "complete" => ReminderAction::Complete,
        Some(v) if v == "reopen" => ReminderAction::Reopen,
        Some(v) if v == "delete" => ReminderAction::Delete,
        _ => return Err("Invalid reminder action.".into()),
    };
    let status = string("status").as_ref().and_then(parse_status);
    Ok(ReminderRequest {
        action,
        id: params.get("id").and_then(JsValue::as_f64).map(|id| id as i64),
        title: string("title"),
        notes: string("notes"),
        project_path: string("projectPath").map(Some),
        due_at: string("dueAt"),
        status,
    })
}

pub fn execute_and_describe(path: Option<&JsString>, request: &ReminderRequest, now: f64) -> Result<JsString, String> {
    let path = path
        .filter(|p| !p.is_empty())
        .ok_or("Reminders require the Pi control database.")?;
    describe_result(request, &execute_reminder_action(path, request, now)?)
}

pub fn execute_reminders_tool(params: &JsValue, path: Option<&JsString>, now: f64) -> Result<JsString, String> {
    execute_and_describe(path, &request_from_tool(params)?, now)
}

pub fn command_notification(args: &JsString, path: Option<&JsString>, now: f64) -> (JsString, &'static str) {
    match parse_reminders_command(args).and_then(|request| execute_and_describe(path, &request, now)) {
        Ok(text) => (text, "info"),
        Err(error) => (error.into(), "error"),
    }
}
