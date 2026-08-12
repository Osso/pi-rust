use pi_protocol::{
    ClientHello, ClientMessage, Command, CreateCommand, CreateCommandInput, ListCommand, ModelRef, PromptCommand,
    RequestEnvelope, SetModelCommand, ThinkingLevel,
};
use serde_json::{Value, json};

use super::{commands::normalize_command, oracle::TsOracle};

const FIRST_REQUEST_ID: u64 = 8026;
const FIXTURE_COUNT: u64 = 6;

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let mut next_id = FIRST_REQUEST_ID;
    compare_request_envelope(oracle, next_id, list_envelope("request-1"));
    next_id += 1;
    compare_request_envelope(oracle, next_id, create_envelope());
    next_id += 1;
    compare_request_envelope(oracle, next_id, prompt_envelope());
    next_id += 1;
    compare_empty_id(oracle, next_id);
    next_id += 1;
    compare_client_message(oracle, next_id, ClientMessage::Hello(ClientHello::new(2)));
    next_id += 1;
    compare_client_message(oracle, next_id, ClientMessage::Request(set_model_envelope()));
    next_id += 1;

    assert_eq!(next_id, FIRST_REQUEST_ID + FIXTURE_COUNT);
}

fn compare_request_envelope(oracle: &mut TsOracle, id: u64, envelope: RequestEnvelope) {
    let value = normalize_request_envelope(&envelope);
    let expected = oracle.request(&json!({
        "id": id,
        "operation": "validate_request_envelope",
        "value": value,
    }));
    let actual = valid_response(id, normalize_request_envelope(&envelope));

    assert_eq!(actual, expected, "request envelope mismatch for id {id}");
}

fn compare_empty_id(oracle: &mut TsOracle, id: u64) {
    let value = json!({"type": "request", "id": "", "request": {"command": "list"}});
    let expected = oracle.request(&json!({
        "id": id,
        "operation": "validate_request_envelope",
        "value": value,
    }));
    let command = Command::List(ListCommand::new());
    let actual = if RequestEnvelope::new(String::new(), command).is_err() {
        invalid_response(id)
    } else {
        panic!("empty request id must be rejected")
    };

    assert_eq!(actual, expected, "empty request id mismatch");
}

fn compare_client_message(oracle: &mut TsOracle, id: u64, message: ClientMessage) {
    let value = normalize_client_message(&message);
    let expected = oracle.request(&json!({
        "id": id,
        "operation": "validate_client_message",
        "value": value,
    }));
    let actual = valid_response(id, normalize_client_message(&message));

    assert_eq!(actual, expected, "client message mismatch for id {id}");
}

fn list_envelope(id: &str) -> RequestEnvelope {
    RequestEnvelope::new(id.to_owned(), Command::List(ListCommand::new())).expect("valid list envelope")
}

fn create_envelope() -> RequestEnvelope {
    let model = ModelRef::new(" provider ".to_owned(), "模型".to_owned()).expect("valid model reference");
    let create = CreateCommand::new(CreateCommandInput {
        cwd: Some(" \tworkspace\0".to_owned()),
        name: Some(String::new()),
        model: Some(model),
        thinking_level: Some(ThinkingLevel::XHigh),
    })
    .expect("valid create command");
    RequestEnvelope::new("要求\0".to_owned(), Command::Create(create)).expect("valid create envelope")
}

fn prompt_envelope() -> RequestEnvelope {
    let prompt = PromptCommand::new("session-é".to_owned(), String::new()).expect("valid prompt command");
    RequestEnvelope::new("request-3".to_owned(), Command::Prompt(prompt)).expect("valid prompt envelope")
}

fn set_model_envelope() -> RequestEnvelope {
    let model = ModelRef::new("anthropic".to_owned(), "claude".to_owned()).expect("valid model reference");
    let command = SetModelCommand::new("session-4".to_owned(), model).expect("valid set-model command");
    RequestEnvelope::new("request-4".to_owned(), Command::SetModel(command)).expect("valid set-model envelope")
}

fn normalize_client_message(message: &ClientMessage) -> Value {
    match message {
        ClientMessage::Hello(hello) => json!({"type": message.as_str(), "version": hello.version()}),
        ClientMessage::Request(envelope) => normalize_request_envelope_with_tag(envelope, message.as_str()),
    }
}

fn normalize_request_envelope(envelope: &RequestEnvelope) -> Value {
    normalize_request_envelope_with_tag(envelope, "request")
}

fn normalize_request_envelope_with_tag(envelope: &RequestEnvelope, tag: &str) -> Value {
    json!({
        "type": tag,
        "id": envelope.id(),
        "request": normalize_command(envelope.request()),
    })
}

fn valid_response(id: u64, value: Value) -> Value {
    json!({"id": id, "ok": true, "output": {"valid": true, "value": value}})
}

fn invalid_response(id: u64) -> Value {
    json!({"id": id, "ok": true, "output": {"valid": false}})
}
