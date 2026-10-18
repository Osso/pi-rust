use pi_protocol::{
    CborValue, ClientHello, ClientMessage, ClientMessageDecoder, Command, FrameDecoder, ListCommand, PROTOCOL_VERSION,
    ProtocolValidationError, RequestEnvelope, ServerHello, ServerMessage, ServerMessageDecoder, ServerSnapshot,
    create_client_message_decoder, create_server_message_decoder, decode_cbor, encode_client_message,
    encode_server_message, is_supported_protocol_version, parse_client_message, parse_server_message,
};

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

fn empty_server_snapshot() -> CborValue {
    object(&[
        ("serverId", text("server-1")),
        ("protocolVersion", number(PROTOCOL_VERSION as f64)),
        ("revision", number(0.0)),
        ("sessions", array(Vec::new())),
        ("models", array(Vec::new())),
    ])
}

fn client_hello(version: f64) -> CborValue {
    object(&[("type", text("hello")), ("version", number(version))])
}

fn server_hello() -> CborValue {
    object(&[
        ("type", text("hello")),
        ("version", number(PROTOCOL_VERSION as f64)),
        ("connectionId", text("connection-1")),
        ("snapshot", empty_server_snapshot()),
    ])
}

fn empty_client_hello() -> ClientMessage {
    ClientMessage::Hello(ClientHello::new(PROTOCOL_VERSION))
}

fn list_request() -> ClientMessage {
    ClientMessage::Request(
        RequestEnvelope::new("request-1".to_string(), Command::List(ListCommand::new()))
            .expect("request id must be non-empty"),
    )
}

fn server_hello_message() -> ServerMessage {
    ServerMessage::Hello(
        ServerHello::new(
            "connection-1".to_string(),
            ServerSnapshot::new("server-1".to_string(), 0, Vec::new(), Vec::new())
                .expect("server id must be non-empty"),
        )
        .expect("connection id must be non-empty"),
    )
}

fn item_message(item: CborValue, progress_type: &str) -> CborValue {
    object(&[
        ("type", text("event")),
        (
            "event",
            object(&[
                ("type", text("session_progress")),
                ("sessionId", text("session-1")),
                ("progress", object(&[("type", text(progress_type)), ("item", item)])),
            ]),
        ),
    ])
}

fn assistant_item(state: &[(&str, CborValue)]) -> CborValue {
    let mut entries = vec![
        ("id", text("assistant-1")),
        ("role", text("assistant")),
        (
            "content",
            array(vec![object(&[("type", text("text")), ("text", text("hello"))])]),
        ),
        ("model", object(&[("provider", text("test")), ("id", text("model"))])),
        ("timestamp", number(1.0)),
    ];
    entries.extend(state.iter().map(|(key, value)| (*key, value.clone())));
    object(&entries)
}

fn tool_item(state: &[(&str, CborValue)]) -> CborValue {
    let mut entries = vec![
        ("id", text("tool-1")),
        ("role", text("tool")),
        ("toolCallId", text("call-1")),
        ("toolName", text("read")),
        ("input", object(&[])),
        ("content", array(Vec::new())),
        ("timestamp", number(1.0)),
    ];
    entries.extend(state.iter().map(|(key, value)| (*key, value.clone())));
    object(&entries)
}

#[test]
fn uses_protocol_version_one() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert!(is_supported_protocol_version(1));
    assert!(!is_supported_protocol_version(2));
}

#[test]
fn accepts_integer_client_hello_versions_for_negotiation() {
    for version in [0.0, 1.0, 2.0] {
        let parsed = parse_client_message(&client_hello(version)).expect("integer version must parse");
        assert_eq!(parsed, ClientMessage::Hello(ClientHello::new(version as u64)));
    }
}

#[test]
fn rejects_invalid_handshakes() {
    for message in [
        object(&[("type", text("hello")), ("version", text("1"))]),
        object(&[("type", text("hello")), ("version", number(1.5))]),
        object(&[
            ("type", text("hello")),
            ("version", number(1.0)),
            ("token", text("secret")),
        ]),
        object(&[
            ("type", text("hello")),
            ("version", number(1.0)),
            ("extra", CborValue::Bool(true)),
        ]),
    ] {
        assert!(parse_client_message(&message).is_err());
    }
}

#[test]
fn does_not_parse_json_strings_as_wire_messages() {
    assert!(parse_client_message(&text("{\"type\":\"hello\",\"version\":1}")).is_err());
    assert!(parse_server_message(&text("{\"type\":\"hello\"}")).is_err());
}

#[test]
fn rejects_image_input_in_prompt_commands() {
    let message = object(&[
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
    assert!(parse_client_message(&message).is_err());
}

#[test]
fn parses_a_server_handshake_snapshot() {
    let message = server_hello();
    let parsed = parse_server_message(&message).expect("server hello must parse");
    assert_eq!(parsed, server_hello_message());
}

#[test]
fn represents_listed_sessions_as_durable_metadata() {
    let message = object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(true)),
        (
            "result",
            object(&[
                ("command", text("list")),
                (
                    "sessions",
                    array(vec![object(&[
                        ("id", text("session-1")),
                        ("createdAt", number(1.0)),
                        ("updatedAt", number(2.0)),
                        ("parentSessionId", text("parent-1")),
                        ("sessionName", text("Named session")),
                        ("cwd", text("/workspace")),
                    ])]),
                ),
            ]),
        ),
    ]);
    assert!(parse_server_message(&message).is_ok());

    let invalid = object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(true)),
        (
            "result",
            object(&[
                ("command", text("list")),
                (
                    "sessions",
                    array(vec![object(&[
                        ("id", text("session-1")),
                        ("createdAt", number(1.0)),
                        ("phase", text("idle")),
                    ])]),
                ),
            ]),
        ),
    ]);
    assert!(parse_server_message(&invalid).is_err());
}

#[test]
fn accepts_supported_error_codes() {
    for code in ["not_implemented", "internal_error"] {
        let message = object(&[
            ("type", text("response")),
            ("id", text("request-1")),
            ("ok", CborValue::Bool(false)),
            ("error", object(&[("code", text(code)), ("message", text("safe"))])),
        ]);
        assert!(parse_server_message(&message).is_ok());
    }
}

#[test]
fn rejects_invalid_server_messages() {
    let messages = [
        object(&[
            ("type", text("hello")),
            ("version", number(2.0)),
            ("connectionId", text("connection-1")),
            ("snapshot", empty_server_snapshot()),
        ]),
        object(&[
            ("type", text("hello_error")),
            (
                "error",
                object(&[("code", text("auth")), ("message", text("Authentication failed"))]),
            ),
        ]),
        object(&[
            ("type", text("response")),
            ("id", text("request-1")),
            ("ok", CborValue::Bool(true)),
            ("result", object(&[("command", text("unknown"))])),
        ]),
        object(&[
            ("type", text("event")),
            (
                "event",
                object(&[("type", text("session_removed")), ("sessionId", number(42.0))]),
            ),
        ]),
    ];
    for message in messages {
        assert!(parse_server_message(&message).is_err());
    }
}

#[test]
fn validates_nested_json_tool_details() {
    let message = object(&[
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
                                (
                                    "details",
                                    object(&[
                                        ("lines", array(vec![number(1.0), number(2.0), number(3.0)])),
                                        ("cached", CborValue::Bool(false)),
                                    ]),
                                ),
                                ("status", text("complete")),
                                ("isError", CborValue::Bool(false)),
                                ("timestamp", number(1.0)),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    assert!(parse_server_message(&message).is_ok());
}

#[test]
fn accepts_consistent_assistant_item_states() {
    let states = [
        vec![("status", text("streaming"))],
        vec![("status", text("complete")), ("stopReason", text("stop"))],
        vec![("status", text("error")), ("stopReason", text("error"))],
        vec![
            ("status", text("error")),
            ("stopReason", text("error")),
            ("errorMessage", text("failed")),
        ],
        vec![("status", text("aborted")), ("stopReason", text("aborted"))],
    ];
    for state in states {
        let streaming = state
            .iter()
            .any(|(key, value)| *key == "status" && value == &text("streaming"));
        let progress = if streaming { "item_updated" } else { "item_finished" };
        let message = item_message(assistant_item(&state), progress);
        assert!(parse_server_message(&message).is_ok(), "{state:?}");
    }
}

#[test]
fn rejects_inconsistent_assistant_item_states() {
    let states = [
        vec![("status", text("streaming")), ("stopReason", text("stop"))],
        vec![("status", text("complete"))],
        vec![("status", text("complete")), ("stopReason", text("error"))],
        vec![
            ("status", text("error")),
            ("stopReason", text("error")),
            ("errorMessage", text("")),
        ],
        vec![("status", text("aborted")), ("stopReason", text("stop"))],
    ];
    for state in states {
        let message = item_message(assistant_item(&state), "item_finished");
        assert!(parse_server_message(&message).is_err(), "{state:?}");
    }
}

#[test]
fn accepts_consistent_tool_item_states() {
    let states = [
        (
            vec![("status", text("running")), ("isError", CborValue::Bool(false))],
            "item_updated",
        ),
        (
            vec![("status", text("complete")), ("isError", CborValue::Bool(false))],
            "item_finished",
        ),
        (
            vec![("status", text("error")), ("isError", CborValue::Bool(true))],
            "item_finished",
        ),
    ];
    for (state, progress) in states {
        let message = item_message(tool_item(&state), progress);
        assert!(parse_server_message(&message).is_ok(), "{state:?}");
    }
}

#[test]
fn rejects_inconsistent_tool_item_states() {
    for state in [
        vec![("status", text("running")), ("isError", CborValue::Bool(true))],
        vec![("status", text("complete")), ("isError", CborValue::Bool(true))],
        vec![("status", text("error")), ("isError", CborValue::Bool(false))],
    ] {
        let message = item_message(tool_item(&state), "item_finished");
        assert!(parse_server_message(&message).is_err(), "{state:?}");
    }
}

#[test]
fn rejects_nonterminal_items_reported_as_finished() {
    let assistant = assistant_item(&[("status", text("streaming"))]);
    let tool = tool_item(&[("status", text("running")), ("isError", CborValue::Bool(false))]);
    assert!(parse_server_message(&item_message(assistant, "item_finished")).is_err());
    assert!(parse_server_message(&item_message(tool, "item_finished")).is_err());
}

#[test]
fn rejects_cbor_byte_strings_in_protocol_values() {
    assert!(parse_client_message(&CborValue::Bytes(vec![1, 2, 3])).is_err());
    let message = object(&[
        ("type", text("response")),
        ("id", text("request-1")),
        ("ok", CborValue::Bool(false)),
        (
            "error",
            object(&[
                ("code", text("invalid_request")),
                ("message", text("invalid")),
                ("details", object(&[("nested", CborValue::Bytes(vec![1, 2, 3]))])),
            ]),
        ),
    ]);
    assert!(parse_server_message(&message).is_err());
}

#[test]
fn encodes_complete_client_and_server_frames() {
    let client_frames = FrameDecoder::new(None)
        .push(&encode_client_message(&empty_client_hello(), None).unwrap())
        .unwrap();
    assert_eq!(client_frames.len(), 1);
    assert_eq!(
        parse_client_message(&decode_cbor(&client_frames[0], None).unwrap()).unwrap(),
        empty_client_hello()
    );

    let server_frames = FrameDecoder::new(None)
        .push(&encode_server_message(&server_hello_message(), None).unwrap())
        .unwrap();
    assert_eq!(server_frames.len(), 1);
    assert_eq!(
        parse_server_message(&decode_cbor(&server_frames[0], None).unwrap()).unwrap(),
        server_hello_message()
    );
}

#[test]
fn enforces_an_outbound_frame_limit_before_returning_encoded_bytes() {
    assert!(encode_client_message(&empty_client_hello(), Some(8)).is_err());
    assert!(encode_server_message(&server_hello_message(), Some(8)).is_err());
}

#[test]
fn omits_absent_optional_properties_on_the_wire() {
    let message = ClientMessage::Request(
        RequestEnvelope::new(
            "request-1".to_string(),
            Command::Create(
                pi_protocol::CreateCommand::new(pi_protocol::CreateCommandInput {
                    cwd: None,
                    name: None,
                    model: None,
                    thinking_level: None,
                })
                .unwrap(),
            ),
        )
        .unwrap(),
    );
    let frames = FrameDecoder::new(None)
        .push(&encode_client_message(&message, None).unwrap())
        .unwrap();
    assert_eq!(
        decode_cbor(&frames[0], None).unwrap(),
        object(&[
            ("type", text("request")),
            ("id", text("request-1")),
            ("request", object(&[("command", text("create"))]),),
        ])
    );
}

#[test]
fn incrementally_decodes_fragmented_and_coalesced_client_messages() {
    let first = encode_client_message(&empty_client_hello(), None).unwrap();
    let second = encode_client_message(&list_request(), None).unwrap();
    let mut wire = first.clone();
    wire.extend_from_slice(&second);

    for split in 0..=wire.len() {
        let mut decoder = ClientMessageDecoder::new(None);
        let mut messages = decoder.push(&wire[..split]).unwrap();
        messages.extend(decoder.push(&wire[split..]).unwrap());
        decoder.end().unwrap();
        assert_eq!(messages, vec![empty_client_hello(), list_request()]);
    }
}

#[test]
fn incrementally_decodes_server_messages() {
    let message = ServerMessage::HelloError(pi_protocol::ServerHelloError::new(pi_protocol::ProtocolError::new(
        pi_protocol::ProtocolErrorCode::Version,
        "Unsupported protocol version".to_string(),
        None,
    )));
    let mut decoder = ServerMessageDecoder::new(None);
    assert_eq!(
        decoder.push(&encode_server_message(&message, None).unwrap()).unwrap(),
        vec![message]
    );
    decoder.end().unwrap();
}

#[test]
fn rejects_invalid_framed_client_input_and_stays_failed() {
    let invalid = [
        pi_protocol::encode_frame(&[]).unwrap(),
        pi_protocol::encode_frame(&[0xff]).unwrap(),
        pi_protocol::encode_frame(
            &pi_protocol::encode_cbor(
                &object(&[
                    ("type", text("hello")),
                    ("version", number(1.0)),
                    ("extra", CborValue::Bool(true)),
                ]),
                None,
            )
            .unwrap(),
        )
        .unwrap(),
    ];

    for wire in invalid {
        let mut decoder = ClientMessageDecoder::new(None);
        assert!(decoder.push(&wire).is_err());
        let error = decoder
            .push(&encode_client_message(&empty_client_hello(), None).unwrap())
            .unwrap_err();
        assert!(error.to_string().to_lowercase().contains("failed"));
    }
}

#[test]
fn rejects_truncated_and_oversized_framing_through_the_validated_decoder() {
    let mut truncated = ServerMessageDecoder::new(None);
    assert!(truncated.push(&[0, 0, 0, 2, 1]).unwrap().is_empty());
    assert!(truncated.end().is_err());

    let mut oversized = ClientMessageDecoder::new(Some(3));
    assert!(oversized.push(&[0, 0, 0, 4]).is_err());
}

#[test]
fn factories_match_the_constructors() {
    let _: ClientMessageDecoder = create_client_message_decoder(None);
    let _: ServerMessageDecoder = create_server_message_decoder(None);
    let _: ProtocolValidationError = ProtocolValidationError::new("message");
}
