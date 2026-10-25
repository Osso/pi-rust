//! Literal JSON ingress for the frozen RPC server. Typed client DTOs are not an input filter.

use super::*;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::JsObject;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use serde::de::DeserializeOwned;

fn type_error(message: impl Into<JsString>) -> CallbackError {
    NativeNodeError::from_constructor(NativeErrorClass::TypeError, message.into()).into()
}

pub(super) fn property<'a>(value: &'a JsValue, key: &str) -> Result<Option<&'a JsValue>, CallbackError> {
    if value == &JsValue::Null {
        return Err(type_error(format!("Cannot read properties of null (reading '{key}')")));
    }
    Ok(value.get(key))
}

fn string_value(value: Option<&JsValue>) -> Result<JsString, CallbackError> {
    value.map_or_else(
        || Ok("undefined".into()),
        |value| js_json::to_js_string(value).map_err(type_error),
    )
}

pub(super) fn caught_message(error: &CallbackError) -> Result<JsString, CallbackError> {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => Ok(error.message),
        AbortReason::Value(value) => string_value(Some(&value)),
    }
}

pub(super) fn response(
    id: Option<JsValue>,
    command: Option<JsValue>,
    success: bool,
    payload: Option<(&str, JsValue)>,
) -> JsValue {
    let mut object = JsObject::new();
    if let Some(id) = id {
        object.insert("id".into(), id);
    }
    object.insert("type".into(), JsValue::String("response".into()));
    if let Some(command) = command {
        object.insert("command".into(), command);
    }
    object.insert("success".into(), JsValue::Bool(success));
    if let Some((key, value)) = payload {
        object.insert(key.into(), value);
    }
    JsValue::Object(object)
}

pub(super) fn command_response(
    command: &JsValue,
    success: bool,
    payload: Option<(&str, JsValue)>,
) -> Result<JsValue, CallbackError> {
    Ok(response(
        property(command, "id")?.cloned(),
        property(command, "type")?.cloned(),
        success,
        payload,
    ))
}

fn boundary_error(field: &str, expected: &str) -> CallbackError {
    type_error(format!(
        "RPC field {field} requires {expected} at the typed core boundary"
    ))
}

fn core_field<T: DeserializeOwned>(
    command: &JsValue,
    key: &str,
    field: &str,
    expected: &str,
) -> Result<T, CallbackError> {
    let value = command.get(key).ok_or_else(|| boundary_error(field, expected))?;
    js_json::from_value(value).map_err(|_| boundary_error(field, expected))
}

fn optional_core_field<T: DeserializeOwned>(
    command: &JsValue,
    key: &str,
    field: &str,
    expected: &str,
) -> Result<Option<T>, CallbackError> {
    command
        .get(key)
        .map(|_| core_field(command, key, field, expected))
        .transpose()
}

fn truthy_string(command: &JsValue, key: &str, field: &str) -> Result<Option<JsString>, CallbackError> {
    match command.get(key).filter(|value| js_json::is_truthy(value)) {
        None => Ok(None),
        Some(value) => value
            .as_string()
            .cloned()
            .map(Some)
            .ok_or_else(|| boundary_error(field, "a string")),
    }
}

fn core_images(
    command: &JsValue,
    field: &str,
    falsey_is_empty: bool,
) -> Result<Option<Vec<pi_ai::types::ImageContent>>, CallbackError> {
    if falsey_is_empty && command.get("images").is_none_or(|value| !js_json::is_truthy(value)) {
        return Ok(None);
    }
    let Some(value) = command.get("images") else {
        return Ok(None);
    };
    let images = value
        .as_array()
        .ok_or_else(|| boundary_error(field, "an image array"))?;
    images
        .iter()
        .enumerate()
        .map(|(index, image)| decode_core_image(image, &format!("{field}[{index}]")))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn decode_core_image(value: &JsValue, field: &str) -> Result<pi_ai::types::ImageContent, CallbackError> {
    value
        .as_object()
        .ok_or_else(|| boundary_error(field, "an image object"))?;
    for key in ["type", "data", "mimeType"] {
        let text = value
            .get(key)
            .and_then(JsValue::as_string)
            .ok_or_else(|| boundary_error(&format!("{field}.{key}"), "a scalar string"))?;
        text.to_utf8().map_err(|_| {
            boundary_error(
                &format!("{field}.{key}"),
                "a scalar string without unpaired UTF-16 units",
            )
        })?;
    }
    js_json::from_value(value).map_err(|_| boundary_error(field, "an image with type=image and string data/mimeType"))
}

pub(super) async fn handle_command(state: &Arc<RpcState>, command: &JsValue) -> Result<Option<JsValue>, CallbackError> {
    property(command, "id")?;
    let raw_type = property(command, "type")?;
    let tag = raw_type
        .and_then(JsValue::as_string)
        .and_then(|name| name.to_utf8().ok());
    let tag = tag.as_deref();
    if tag == Some("prompt") {
        let message = core_field(command, "message", "prompt.message", "a string")?;
        let images = core_images(command, "prompt.images", false)?;
        let streaming = optional_core_field(
            command,
            "streamingBehavior",
            "prompt.streamingBehavior",
            "steer or followUp",
        )?;
        start_prompt(
            state,
            command.get("id").cloned(),
            message,
            images,
            streaming.map(to_session_streaming_behavior),
        );
        return Ok(None);
    }
    let session = state.current_session();
    let data = match tag {
        Some(tag @ ("steer" | "follow_up" | "abort" | "interrupt")) => {
            dispatch_prompting(&session, command, tag).await?
        }
        Some(tag @ ("new_session" | "switch_session" | "fork" | "clone")) => {
            dispatch_transition(state, &session, command, tag).await?
        }
        Some(
            tag
            @ ("set_model" | "cycle_model" | "get_available_models" | "set_thinking_level" | "cycle_thinking_level"),
        ) => dispatch_model(&session, command, tag).await?,
        Some(
            tag @ ("set_steering_mode"
            | "set_follow_up_mode"
            | "set_auto_compaction"
            | "set_auto_retry"
            | "set_session_name"),
        ) => dispatch_settings(&session, command, tag)?,
        Some(tag @ ("compact" | "bash" | "abort_retry" | "abort_bash" | "export_html")) => {
            dispatch_operation(&session, command, tag).await?
        }
        Some(
            tag @ ("get_state"
            | "get_session_stats"
            | "get_fork_messages"
            | "get_entries"
            | "get_tree"
            | "get_last_assistant_text"
            | "get_messages"
            | "get_commands"),
        ) => dispatch_query(&session, command, tag)?,
        _ => {
            let error = JsString::from("Unknown command: ") + string_value(raw_type)?;
            return command_response(command, false, Some(("error", JsValue::String(error)))).map(Some);
        }
    };
    command_response(command, true, data.map(|data| ("data", data))).map(Some)
}

async fn dispatch_prompting(
    session: &AgentSession,
    command: &JsValue,
    tag: &str,
) -> Result<Option<JsValue>, CallbackError> {
    match tag {
        "steer" | "follow_up" => {
            let field = format!("{tag}.message");
            let message: JsString = core_field(command, "message", &field, "a string")?;
            let images = core_images(command, &format!("{tag}.images"), true)?;
            let images = images.as_deref().unwrap_or_default();
            if tag == "steer" {
                session.steer(&message, images).await?;
            } else {
                session.follow_up(&message, images).await?;
            }
        }
        "abort" => session.abort().await?,
        "interrupt" => {
            let (steering, follow_up) = session.clear_queue();
            let message = JsString::join(steering.into_iter().chain(follow_up), "\n\n");
            let session = session.clone();
            tokio::task::spawn_local(async move {
                if let Err(error) = session.interrupt(&message, InputSource::Rpc).await {
                    eprintln!("RPC interrupt failed: {}", encode_node_utf8(error.message()));
                }
            });
        }
        _ => unreachable!("prompting dispatch is selected above"),
    }
    Ok(None)
}

async fn dispatch_transition(
    state: &Arc<RpcState>,
    session: &AgentSession,
    command: &JsValue,
    tag: &str,
) -> Result<Option<JsValue>, CallbackError> {
    let result = match tag {
        "new_session" => {
            let parent_session = truthy_string(command, "parentSession", "new_session.parentSession")?;
            let result = state
                .runtime
                .new_session(RuntimeNewSessionOptions {
                    parent_session,
                    setup: None,
                    with_session: None,
                })
                .await?;
            (result.cancelled, pi_js_compat::js_json!({"cancelled":result.cancelled}))
        }
        "switch_session" => {
            let path = core_field(command, "sessionPath", "switch_session.sessionPath", "a string")?;
            let result = state.runtime.switch_session(&path, Default::default()).await?;
            (result.cancelled, pi_js_compat::js_json!({"cancelled":result.cancelled}))
        }
        "fork" => {
            let entry = core_field(command, "entryId", "fork.entryId", "a string")?;
            let result = state.runtime.fork(&entry, ForkPosition::Before, None).await?;
            let data = js_json::to_value(&ForkResponseData {
                text: result.selected_text,
                cancelled: result.cancelled,
            })
            .expect("fork data must serialize");
            (result.cancelled, data)
        }
        "clone" => {
            let leaf = session
                .session_manager()
                .get_leaf_id()
                .cloned()
                .ok_or_else(|| CallbackError::from("Cannot clone session: no current entry selected"))?;
            let result = state.runtime.fork(&leaf, ForkPosition::At, None).await?;
            (result.cancelled, pi_js_compat::js_json!({"cancelled":result.cancelled}))
        }
        _ => unreachable!("transition dispatch is selected above"),
    };
    if !result.0 {
        rebind_session(state).await?;
    }
    Ok(Some(result.1))
}

async fn dispatch_model(
    session: &AgentSession,
    command: &JsValue,
    tag: &str,
) -> Result<Option<JsValue>, CallbackError> {
    match tag {
        "set_model" => {
            let available = session
                .model_registry()
                .get_available()
                .map_err(|error| error.message)?;
            let provider = command.get("provider");
            let id = command.get("modelId");
            let selected = available.into_iter().find(|model| {
                provider == Some(&JsValue::String(model.provider.clone().into()))
                    && id == Some(&JsValue::String(model.id.clone().into()))
            });
            let Some(selected) = selected else {
                return Err(
                    (JsString::from("Model not found: ") + string_value(provider)? + "/" + string_value(id)?).into(),
                );
            };
            session.set_model(selected.clone()).await?;
            Ok(Some(js_json::to_value(&selected).expect("model must serialize")))
        }
        "cycle_model" => Ok(Some(
            session
                .cycle_model(CycleDirection::Forward)
                .await?
                .map_or(JsValue::Null, |result| {
                    js_json::to_value(&result).expect("cycle result must serialize")
                }),
        )),
        "get_available_models" => {
            let models = session
                .model_registry()
                .get_available()
                .map_err(|error| error.message)?;
            Ok(Some(pi_js_compat::js_json!({"models":models})))
        }
        "set_thinking_level" => {
            session.set_thinking_level(core_field(
                command,
                "level",
                "set_thinking_level.level",
                "a thinking level",
            )?)?;
            Ok(None)
        }
        "cycle_thinking_level" => Ok(Some(
            session
                .cycle_thinking_level()?
                .map_or(JsValue::Null, |level| pi_js_compat::js_json!({"level":level})),
        )),
        _ => unreachable!("model dispatch is selected above"),
    }
}

fn dispatch_settings(session: &AgentSession, command: &JsValue, tag: &str) -> Result<Option<JsValue>, CallbackError> {
    match tag {
        "set_steering_mode" => session.set_steering_mode(core_field(
            command,
            "mode",
            "set_steering_mode.mode",
            "all or one-at-a-time",
        )?),
        "set_follow_up_mode" => session.set_follow_up_mode(core_field(
            command,
            "mode",
            "set_follow_up_mode.mode",
            "all or one-at-a-time",
        )?),
        "set_auto_compaction" => session.set_auto_compaction_enabled(core_field(
            command,
            "enabled",
            "set_auto_compaction.enabled",
            "a boolean",
        )?),
        "set_auto_retry" => {
            session.set_auto_retry_enabled(core_field(command, "enabled", "set_auto_retry.enabled", "a boolean")?)
        }
        "set_session_name" => {
            let name = match command.get("name") {
                Some(JsValue::String(name)) => name.trim(),
                None => return Err(type_error("Cannot read properties of undefined (reading 'trim')")),
                Some(JsValue::Null) => return Err(type_error("Cannot read properties of null (reading 'trim')")),
                Some(_) => return Err(type_error("command.name.trim is not a function")),
            };
            if name.is_empty() {
                return Err("Session name cannot be empty".into());
            }
            session
                .set_session_name(&name)
                .map_err(|error| error.message().to_owned())?;
        }
        _ => unreachable!("settings dispatch is selected above"),
    }
    Ok(None)
}

async fn dispatch_operation(
    session: &AgentSession,
    command: &JsValue,
    tag: &str,
) -> Result<Option<JsValue>, CallbackError> {
    match tag {
        "compact" => {
            let custom = optional_core_field(command, "customInstructions", "compact.customInstructions", "a string")?;
            let result: CompactionResult = session.compact(custom.as_ref()).await?;
            Ok(Some(js_json::to_value(&result).expect("compaction must serialize")))
        }
        "bash" => {
            let text = core_field(command, "command", "bash.command", "a string")?;
            let exclude_from_context =
                optional_core_field(command, "excludeFromContext", "bash.excludeFromContext", "a boolean")?;
            let result = session
                .execute_bash(
                    &text,
                    None,
                    ExecuteBashOptions {
                        exclude_from_context,
                        ..Default::default()
                    },
                )
                .await?;
            Ok(Some(js_json::to_value(&result).expect("bash must serialize")))
        }
        "abort_retry" => {
            session.abort_retry();
            Ok(None)
        }
        "abort_bash" => {
            session.abort_bash();
            Ok(None)
        }
        "export_html" => {
            let output = truthy_string(command, "outputPath", "export_html.outputPath")?;
            let path = session.export_to_html(output.as_ref()).await?;
            Ok(Some(pi_js_compat::js_json!({"path":path})))
        }
        _ => unreachable!("operation dispatch is selected above"),
    }
}

fn dispatch_query(session: &AgentSession, command: &JsValue, tag: &str) -> Result<Option<JsValue>, CallbackError> {
    let data = match tag {
        "get_state" => js_json::to_value(&session_state(session)).expect("state must serialize"),
        "get_session_stats" => js_json::to_value(&session.get_session_stats()).expect("stats must serialize"),
        "get_fork_messages" => {
            let messages: Vec<_> = session
                .get_user_messages_for_forking()
                .into_iter()
                .map(|message| ForkMessageData {
                    entry_id: message.entry_id,
                    text: message.text,
                })
                .collect();
            pi_js_compat::js_json!({"messages":messages})
        }
        "get_entries" => entries(session, command)?,
        "get_tree" => {
            let manager = session.session_manager();
            js_json::to_value(&TreeResponseData {
                tree: manager.get_tree(),
                leaf_id: manager.get_leaf_id().cloned(),
            })
            .expect("tree must serialize")
        }
        "get_last_assistant_text" => js_json::to_value(&LastAssistantTextData {
            text: session.get_last_assistant_text(),
        })
        .expect("text must serialize"),
        "get_messages" => pi_js_compat::js_json!({"messages":session.messages()}),
        "get_commands" => pi_js_compat::js_json!({"commands":collect_commands(session)}),
        _ => unreachable!("query dispatch is selected above"),
    };
    Ok(Some(data))
}

fn entries(session: &AgentSession, command: &JsValue) -> Result<JsValue, CallbackError> {
    let manager = session.session_manager();
    let mut entries = manager.get_entries();
    if let Some(since) = command.get("since") {
        let index = entries
            .iter()
            .position(|entry| entry.id().is_some_and(|id| since == &JsValue::String(id.clone())));
        let Some(index) = index else {
            return Err((JsString::from("Entry not found: ") + string_value(Some(since))?).into());
        };
        entries = entries.into_iter().skip(index + 1).collect();
    }
    js_json::to_value(&EntriesResponseData {
        entries,
        leaf_id: manager.get_leaf_id().cloned(),
    })
    .map_err(|error| type_error(error.to_string()))
}
