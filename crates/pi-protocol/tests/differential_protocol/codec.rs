use super::cbor::{hex_encode, tagged};
use super::oracle::TsOracle;
use pi_protocol::{
    CborValue, ClientMessage, ServerMessage, encode_client_message, encode_server_message, parse_client_message,
    parse_server_message,
};
use serde_json::{Value, json};

enum Operation {
    EncodeClient {
        value: CborValue,
        max_frame_length: Option<u32>,
    },
    EncodeServer {
        value: CborValue,
        max_frame_length: Option<u32>,
    },
    ParseClient {
        value: CborValue,
    },
    ParseServer {
        value: CborValue,
    },
}

struct Fixture {
    name: String,
    operation: Operation,
}

impl Fixture {
    fn request(&self, id: u64) -> Value {
        match &self.operation {
            Operation::EncodeClient {
                value,
                max_frame_length,
            } => message_request(id, "encode_client_message", value, *max_frame_length),
            Operation::EncodeServer {
                value,
                max_frame_length,
            } => message_request(id, "encode_server_message", value, *max_frame_length),
            Operation::ParseClient { value } => json!({
                "id": id,
                "operation": "parse_client_message",
                "value": tagged(value),
            }),
            Operation::ParseServer { value } => json!({
                "id": id,
                "operation": "parse_server_message",
                "value": tagged(value),
            }),
        }
    }

    fn rust_response(&self, id: u64) -> Value {
        match &self.operation {
            Operation::EncodeClient {
                value,
                max_frame_length,
            } => encode_client_response(id, value, *max_frame_length),
            Operation::EncodeServer {
                value,
                max_frame_length,
            } => encode_server_response(id, value, *max_frame_length),
            Operation::ParseClient { value } => parse_client_response(id, value),
            Operation::ParseServer { value } => parse_server_response(id, value),
        }
    }
}

fn message_request(id: u64, operation: &str, value: &CborValue, max_frame_length: Option<u32>) -> Value {
    let mut request = json!({"id": id, "operation": operation, "value": tagged(value)});
    if let Some(maximum) = max_frame_length {
        request["max_frame_length"] = json!(maximum);
    }
    request
}

fn encode_client_response(id: u64, value: &CborValue, max_frame_length: Option<u32>) -> Value {
    match parse_client_message(value) {
        Ok(message) => encode_client_message_result(id, &message, max_frame_length),
        Err(error) => validation_error(id, &error),
    }
}

fn encode_client_message_result(id: u64, message: &ClientMessage, max_frame_length: Option<u32>) -> Value {
    match encode_client_message(message, max_frame_length) {
        Ok(frame) => encoded_message(id, &frame),
        Err(error) => validation_error(id, &error),
    }
}

fn encode_server_response(id: u64, value: &CborValue, max_frame_length: Option<u32>) -> Value {
    match parse_server_message(value) {
        Ok(message) => encode_server_message_result(id, &message, max_frame_length),
        Err(error) => validation_error(id, &error),
    }
}

fn encode_server_message_result(id: u64, message: &ServerMessage, max_frame_length: Option<u32>) -> Value {
    match encode_server_message(message, max_frame_length) {
        Ok(frame) => encoded_message(id, &frame),
        Err(error) => validation_error(id, &error),
    }
}

fn parse_client_response(id: u64, value: &CborValue) -> Value {
    match parse_client_message(value) {
        Ok(_) => parsed_value(id, value),
        Err(error) => validation_error(id, &error),
    }
}

fn parse_server_response(id: u64, value: &CborValue) -> Value {
    match parse_server_message(value) {
        Ok(_) => parsed_value(id, value),
        Err(error) => validation_error(id, &error),
    }
}

fn encoded_message(id: u64, frame: &[u8]) -> Value {
    json!({"id": id, "ok": true, "output": {"message_hex": hex_encode(frame)}})
}

fn parsed_value(id: u64, value: &CborValue) -> Value {
    json!({"id": id, "ok": true, "output": {"value": tagged(value)}})
}

fn validation_error(id: u64, error: &pi_protocol::ProtocolValidationError) -> Value {
    json!({
        "id": id,
        "ok": false,
        "error": {"name": "ProtocolValidationError", "message": error.to_string()},
    })
}

fn text(value: &str) -> CborValue {
    CborValue::Text(value.to_string())
}

fn number(value: f64) -> CborValue {
    CborValue::Number(value)
}

fn object(entries: &[(&str, CborValue)]) -> CborValue {
    CborValue::Map(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect(),
    )
}

fn array(values: Vec<CborValue>) -> CborValue {
    CborValue::Array(values)
}

fn client_hello(version: f64) -> CborValue {
    object(&[("type", text("hello")), ("version", number(version))])
}

fn empty_server_snapshot() -> CborValue {
    object(&[
        ("serverId", text("server-1")),
        ("protocolVersion", number(1.0)),
        ("revision", number(0.0)),
        ("sessions", array(Vec::new())),
        ("models", array(Vec::new())),
    ])
}

fn server_hello() -> CborValue {
    object(&[
        ("type", text("hello")),
        ("version", number(1.0)),
        ("connectionId", text("connection-1")),
        ("snapshot", empty_server_snapshot()),
    ])
}

fn session_metadata() -> CborValue {
    object(&[
        ("id", text("session-1")),
        ("createdAt", number(1.0)),
        ("updatedAt", number(2.0)),
        ("parentSessionId", text("parent-1")),
        ("sessionName", text("Named session")),
        ("cwd", text("/workspace")),
    ])
}

fn list_response() -> CborValue {
    object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(true)),
        (
            "result",
            object(&[("command", text("list")), ("sessions", array(vec![session_metadata()]))]),
        ),
    ])
}

fn error_response() -> CborValue {
    object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(false)),
        (
            "error",
            object(&[
                ("code", text("invalid_request")),
                ("message", text("invalid")),
                (
                    "details",
                    object(&[("nested", array(vec![CborValue::Null, CborValue::Bool(true)]))]),
                ),
            ]),
        ),
    ])
}

fn tool_finished_event() -> CborValue {
    object(&[
        ("type", text("event")),
        (
            "event",
            object(&[
                ("type", text("session_progress")),
                ("sessionId", text("session-1")),
                (
                    "progress",
                    object(&[
                        ("type", text("item_finished")),
                        (
                            "item",
                            object(&[
                                ("id", text("tool-1")),
                                ("role", text("tool")),
                                ("toolCallId", text("call-1")),
                                ("toolName", text("read")),
                                ("input", object(&[("path", text("/tmp/file"))])),
                                (
                                    "content",
                                    array(vec![object(&[("type", text("text")), ("text", text("done"))])]),
                                ),
                                ("details", object(&[("lines", array(vec![number(1.0), number(2.0)]))])),
                                ("timestamp", number(1.0)),
                                ("status", text("complete")),
                                ("isError", CborValue::Bool(false)),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ])
}

fn encode_client(name: &str, value: CborValue, max_frame_length: Option<u32>) -> Fixture {
    Fixture {
        name: name.to_string(),
        operation: Operation::EncodeClient {
            value,
            max_frame_length,
        },
    }
}

fn encode_server(name: &str, value: CborValue, max_frame_length: Option<u32>) -> Fixture {
    Fixture {
        name: name.to_string(),
        operation: Operation::EncodeServer {
            value,
            max_frame_length,
        },
    }
}

fn parse_client(name: &str, value: CborValue) -> Fixture {
    Fixture {
        name: name.to_string(),
        operation: Operation::ParseClient { value },
    }
}

fn parse_server(name: &str, value: CborValue) -> Fixture {
    Fixture {
        name: name.to_string(),
        operation: Operation::ParseServer { value },
    }
}

fn fixtures() -> Vec<Fixture> {
    let create_request = object(&[
        ("type", text("request")),
        ("id", text("request-1")),
        (
            "request",
            object(&[
                ("command", text("create")),
                ("cwd", text("/workspace")),
                ("name", text("named")),
                ("model", object(&[("provider", text("test")), ("id", text("model"))])),
                ("thinkingLevel", text("low")),
            ]),
        ),
    ]);
    let prompt_request = object(&[
        ("type", text("request")),
        ("id", text("request-1")),
        (
            "request",
            object(&[
                ("command", text("prompt")),
                ("sessionId", text("session-1")),
                ("text", text("inspect")),
            ]),
        ),
    ]);
    let set_thinking_request = object(&[
        ("type", text("request")),
        ("id", text("request-1")),
        (
            "request",
            object(&[
                ("command", text("set_thinking")),
                ("sessionId", text("session-1")),
                ("thinkingLevel", text("max")),
            ]),
        ),
    ]);
    let list_request = object(&[
        ("type", text("request")),
        ("id", text("request-1")),
        ("request", object(&[("command", text("list"))])),
    ]);
    let invalid_prompt = object(&[
        ("type", text("request")),
        ("id", text("request-1")),
        (
            "request",
            object(&[
                ("command", text("prompt")),
                ("sessionId", text("session-1")),
                ("text", text("inspect")),
                (
                    "images",
                    array(vec![object(&[
                        ("type", text("image")),
                        ("data", text("abc")),
                        ("mimeType", text("image/png")),
                    ])]),
                ),
            ]),
        ),
    ]);

    let hello_error = object(&[
        ("type", text("hello_error")),
        (
            "error",
            object(&[
                ("code", text("version")),
                ("message", text("Unsupported protocol version")),
            ]),
        ),
    ]);
    let session_removed = object(&[
        ("type", text("event")),
        (
            "event",
            object(&[("type", text("session_removed")), ("sessionId", text("session-1"))]),
        ),
    ]);
    let invalid_hello = object(&[
        ("type", text("hello")),
        ("version", number(2.0)),
        ("connectionId", text("connection-1")),
        ("snapshot", empty_server_snapshot()),
    ]);
    let invalid_auth = object(&[
        ("type", text("hello_error")),
        (
            "error",
            object(&[("code", text("auth")), ("message", text("Authentication failed"))]),
        ),
    ]);
    let invalid_command = object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(true)),
        ("result", object(&[("command", text("unknown"))])),
    ]);
    let invalid_session_id = object(&[
        ("type", text("event")),
        (
            "event",
            object(&[("type", text("session_removed")), ("sessionId", number(42.0))]),
        ),
    ]);
    let invalid_tool_state = object(&[
        ("type", text("event")),
        (
            "event",
            object(&[
                ("type", text("session_progress")),
                ("sessionId", text("session-1")),
                (
                    "progress",
                    object(&[
                        ("type", text("item_finished")),
                        (
                            "item",
                            object(&[
                                ("id", text("tool-1")),
                                ("role", text("tool")),
                                ("toolCallId", text("call-1")),
                                ("toolName", text("read")),
                                ("input", object(&[])),
                                ("content", array(Vec::new())),
                                ("timestamp", number(1.0)),
                                ("status", text("running")),
                                ("isError", CborValue::Bool(false)),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);

    vec![
        encode_client("encode client hello", client_hello(1.0), None),
        encode_client("encode client hello version 2", client_hello(2.0), None),
        encode_client("encode list request", list_request.clone(), None),
        encode_client("encode create request", create_request.clone(), None),
        encode_client("encode prompt request", prompt_request.clone(), None),
        encode_client("encode set-thinking request", set_thinking_request.clone(), None),
        encode_client(
            "encode invalid hello version",
            object(&[("type", text("hello")), ("version", text("1"))]),
            None,
        ),
        encode_client(
            "encode hello with extra field",
            object(&[
                ("type", text("hello")),
                ("version", number(1.0)),
                ("extra", CborValue::Bool(true)),
            ]),
            None,
        ),
        encode_client("encode prompt with images", invalid_prompt.clone(), None),
        encode_client("encode client hello under limit", client_hello(1.0), Some(8)),
        encode_server("encode server hello", server_hello(), None),
        encode_server("encode server hello error", hello_error.clone(), None),
        encode_server("encode list response", list_response(), None),
        encode_server("encode error response", error_response(), None),
        encode_server("encode session removed", session_removed.clone(), None),
        encode_server("encode tool finished event", tool_finished_event(), None),
        encode_server("encode server hello under limit", server_hello(), Some(8)),
        parse_client("parse client hello", client_hello(1.0)),
        parse_client("parse list request", list_request),
        parse_client("parse create request", create_request),
        parse_client("parse prompt request", prompt_request),
        parse_client("parse set-thinking request", set_thinking_request),
        parse_client(
            "parse invalid hello version",
            object(&[("type", text("hello")), ("version", text("1"))]),
        ),
        parse_client("parse prompt with images", invalid_prompt),
        parse_client("parse byte string message", CborValue::Bytes(vec![1, 2, 3])),
        parse_server("parse server hello", server_hello()),
        parse_server("parse server hello error", hello_error),
        parse_server("parse list response", list_response()),
        parse_server("parse error response", error_response()),
        parse_server("parse session removed", session_removed),
        parse_server("parse tool finished event", tool_finished_event()),
        parse_server("parse invalid hello version", invalid_hello),
        parse_server("parse invalid auth code", invalid_auth),
        parse_server("parse invalid command", invalid_command),
        parse_server("parse invalid session id", invalid_session_id),
        parse_server("parse invalid tool state", invalid_tool_state),
        parse_server("parse byte string message", CborValue::Bytes(vec![1, 2, 3])),
    ]
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    for (index, fixture) in fixtures().iter().enumerate() {
        let id = u64::try_from(index + 1).expect("fixture index must fit in request id");
        let expected = oracle.request(&fixture.request(id));
        let actual = fixture.rust_response(id);
        assert_eq!(actual, expected, "differential mismatch: {}", fixture.name);
    }
}
