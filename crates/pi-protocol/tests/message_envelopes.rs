use pi_protocol::{ClientHello, ClientMessage, Command, PromptCommand, RequestEnvelope, RequestEnvelopeError};

#[test]
fn request_envelope_preserves_non_empty_id_and_nested_prompt_text() {
    let prompt = PromptCommand::new("session-1".to_owned(), String::new()).expect("valid prompt command");
    let envelope =
        RequestEnvelope::new("模型\0".to_owned(), Command::Prompt(prompt.clone())).expect("valid request envelope");

    assert_eq!(envelope.id(), "模型\0");
    assert_eq!(envelope.request(), &Command::Prompt(prompt));
}

#[test]
fn request_envelope_rejects_empty_id_with_exact_error() {
    let prompt = PromptCommand::new("session-1".to_owned(), String::new()).expect("valid prompt command");
    let error = RequestEnvelope::new(String::new(), Command::Prompt(prompt))
        .expect_err("empty request envelope id must be rejected");

    assert_eq!(error, RequestEnvelopeError::EmptyId);
    assert_eq!(error.to_string(), "Request envelope id must not be empty");
}

#[test]
fn client_message_preserves_hello_and_request_variants_with_exact_tags() {
    let hello = ClientHello::new(1);
    let hello_message = ClientMessage::Hello(hello);
    assert_eq!(hello_message.as_str(), "hello");
    match hello_message {
        ClientMessage::Hello(value) => assert_eq!(value, hello),
        ClientMessage::Request(_) => panic!("expected hello message"),
    }

    let prompt = PromptCommand::new("session-1".to_owned(), String::new()).expect("valid prompt command");
    let envelope =
        RequestEnvelope::new("request-1".to_owned(), Command::Prompt(prompt)).expect("valid request envelope");
    let request_message = ClientMessage::Request(envelope.clone());
    assert_eq!(request_message.as_str(), "request");
    match request_message {
        ClientMessage::Hello(_) => panic!("expected request message"),
        ClientMessage::Request(value) => assert_eq!(value, envelope),
    }
}
