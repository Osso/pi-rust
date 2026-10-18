//! Port of `packages/coding-agent/src/core/permissions/mcp-permission-prompt.ts`.

use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::core::desktop_notification::{
    DesktopNotification, DesktopNotificationHandle, DesktopNotifier, PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS,
};
use crate::core::extensions::types::{ToolCallEvent, ToolCallEventResult};
use crate::core::rule_store::{
    PermissionRuleDestination, PermissionRuleStore, PermissionRuleUpdate, build_permission_rule_content,
};

/// Arguments forwarded to the MCP permission-prompt tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionPromptInput {
    pub tool_name: String,
    pub input: Map<String, Value>,
    pub tool_use_id: String,
    pub cwd: String,
}

/// Calls the MCP permission-prompt tool. `Err` maps a rejected promise.
pub type PermissionPromptCaller =
    Arc<dyn for<'a> Fn(&'a str, PermissionPromptInput) -> BoxFuture<'a, Result<Value, String>> + Send + Sync>;

/// Parsed permission-prompt response.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionPromptDecision {
    Allow {
        updated_input: Option<Map<String, Value>>,
        updated_permissions: Option<Vec<PermissionRuleUpdate>>,
    },
    Deny {
        message: String,
    },
}

/// Inputs for [`create_permission_prompt_handler`].
#[derive(Clone)]
pub struct PermissionPromptHandlerOptions {
    pub permission_prompt_tool: Option<String>,
    pub cwd: String,
    pub call_tool: PermissionPromptCaller,
    pub desktop_notifier: Option<Arc<dyn DesktopNotifier>>,
    pub rule_store: Option<Arc<Mutex<PermissionRuleStore>>>,
}

/// Tool-call handler produced by [`create_permission_prompt_handler`].
pub type PermissionPromptHandler =
    Arc<dyn for<'a> Fn(&'a mut ToolCallEvent) -> BoxFuture<'a, Option<ToolCallEventResult>> + Send + Sync>;

fn mcp_tool_name_pattern() -> &'static Regex {
    static PATTERN: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^mcp__[A-Za-z0-9_-]+__[A-Za-z0-9_-]+$").expect("valid pattern"))
}

fn is_record(value: &Value) -> bool {
    value.is_object()
}

fn parse_json_object(json: &str) -> Option<Map<String, Value>> {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|value| value.as_object().cloned())
}

fn extract_text_content(response: &Map<String, Value>) -> Option<String> {
    let content = response.get("content")?.as_array()?;
    content.iter().find_map(|item| {
        let item = item.as_object()?;
        if item.get("type").and_then(Value::as_str) != Some("text") {
            return None;
        }
        item.get("text").and_then(Value::as_str).map(str::to_owned)
    })
}

fn parse_response_object(raw_response: &Value) -> Option<Map<String, Value>> {
    if let Value::String(text) = raw_response {
        return parse_json_object(text);
    }
    if is_record(raw_response) {
        let object = raw_response.as_object()?;
        if let Some(text) = extract_text_content(object) {
            return parse_json_object(&text);
        }
        return Some(object.clone());
    }
    None
}

fn parse_updated_permissions(value: Option<&Value>) -> Option<Vec<PermissionRuleUpdate>> {
    let value = value?;
    let updates = value.as_array()?;
    let mut parsed = Vec::new();
    for update in updates {
        let update = update.as_object()?;
        if update.get("type").and_then(Value::as_str) != Some("addRules")
            || update.get("behavior").and_then(Value::as_str) != Some("allow")
        {
            return None;
        }
        let destination = parse_rule_destination(update.get("destination")?)?;
        let rules = update.get("rules")?.as_array()?;
        let mut parsed_rules = Vec::new();
        for rule in rules {
            parsed_rules.push(rule.as_str()?.to_owned());
        }
        parsed.push(PermissionRuleUpdate::new(destination, parsed_rules));
    }
    Some(parsed)
}

fn parse_rule_destination(value: &Value) -> Option<PermissionRuleDestination> {
    match value.as_str()? {
        "session" => Some(PermissionRuleDestination::Session),
        "userSettings" => Some(PermissionRuleDestination::UserSettings),
        "projectSettings" => Some(PermissionRuleDestination::ProjectSettings),
        "localSettings" => Some(PermissionRuleDestination::LocalSettings),
        _ => None,
    }
}

/// Parse a permission-prompt response into a decision.
pub fn parse_permission_prompt_decision(raw_response: &Value) -> Option<PermissionPromptDecision> {
    let response = parse_response_object(raw_response)?;
    match response.get("behavior").and_then(Value::as_str) {
        Some("allow") => {
            let updated_permissions_present = response.contains_key("updatedPermissions");
            let updated_permissions = parse_updated_permissions(response.get("updatedPermissions"));
            if updated_permissions_present && updated_permissions.is_none() {
                return None;
            }
            let updated_input = match response.get("updatedInput") {
                None => None,
                Some(Value::Object(input)) => Some(input.clone()),
                Some(_) => return None,
            };
            Some(PermissionPromptDecision::Allow {
                updated_input,
                updated_permissions,
            })
        }
        Some("deny") => {
            let message = response.get("message")?.as_str()?;
            if message.is_empty() {
                return None;
            }
            Some(PermissionPromptDecision::Deny {
                message: message.to_owned(),
            })
        }
        _ => None,
    }
}

fn notify_permission_prompt(
    event: &ToolCallEvent,
    cwd: &str,
    desktop_notifier: Option<&Arc<dyn DesktopNotifier>>,
) -> Option<Box<dyn DesktopNotificationHandle>> {
    let desktop_notifier = desktop_notifier?;
    desktop_notifier.notify(&DesktopNotification {
        body: format!("Permission approval needed for {} in {cwd}.", event.tool_name.as_str()),
        expire_time_ms: Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
        title: "Pi permission approval needed".to_owned(),
        urgency: None,
    })
}

fn replace_input_in_place(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    target.clear();
    target.extend(source.clone());
}

/// Create a handler that asks an MCP tool for permission decisions.
pub fn create_permission_prompt_handler(options: PermissionPromptHandlerOptions) -> PermissionPromptHandler {
    Arc::new(move |event: &mut ToolCallEvent| {
        let options = options.clone();
        Box::pin(async move {
            let permission_prompt_tool = options.permission_prompt_tool.as_deref()?;
            if !mcp_tool_name_pattern().is_match(permission_prompt_tool) {
                return None;
            }

            let rule_content = build_permission_rule_content(event.tool_name.as_str(), &event.input);
            if let Some(rule_store) = &options.rule_store
                && rule_store
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .has_allow_rule(event.tool_name.as_str(), &rule_content)
            {
                return None;
            }

            let notification_handle = notify_permission_prompt(event, &options.cwd, options.desktop_notifier.as_ref());

            let response = (options.call_tool)(
                permission_prompt_tool,
                PermissionPromptInput {
                    tool_name: event.tool_name.as_str().to_owned(),
                    input: event.input.clone(),
                    tool_use_id: event.tool_call_id.clone(),
                    cwd: options.cwd.clone(),
                },
            )
            .await;

            if let Some(handle) = notification_handle {
                handle.close();
            }

            let decision = match response {
                Ok(response) => parse_permission_prompt_decision(&response),
                Err(_) => None,
            };
            let decision = decision?;

            match decision {
                PermissionPromptDecision::Deny { message } => Some(ToolCallEventResult {
                    block: Some(true),
                    reason: Some(message),
                    terminate: None,
                }),
                PermissionPromptDecision::Allow {
                    updated_input,
                    updated_permissions,
                } => {
                    if let Some(rule_store) = &options.rule_store {
                        rule_store
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .apply_updated_permissions(event.tool_name.as_str(), updated_permissions.as_deref());
                    }
                    if let Some(updated_input) = updated_input {
                        replace_input_in_place(&mut event.input, &updated_input);
                    }
                    None
                }
            }
        }) as BoxFuture<'_, Option<ToolCallEventResult>>
    })
}
