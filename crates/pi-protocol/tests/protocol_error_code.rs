use pi_protocol::{ParseProtocolErrorCodeError, ProtocolErrorCode};

#[test]
fn protocol_error_codes_round_trip_exact_source_literals() {
    let cases = [
        ("version", ProtocolErrorCode::Version),
        ("busy", ProtocolErrorCode::Busy),
        ("session_locked", ProtocolErrorCode::SessionLocked),
        ("not_found", ProtocolErrorCode::NotFound),
        ("invalid_request", ProtocolErrorCode::InvalidRequest),
        ("not_implemented", ProtocolErrorCode::NotImplemented),
        ("internal_error", ProtocolErrorCode::InternalError),
    ];

    for (literal, expected) in cases {
        let code = literal
            .parse::<ProtocolErrorCode>()
            .expect("source protocol error-code literals must parse");
        assert_eq!(code, expected);
        assert_eq!(code.as_str(), literal);
    }
}

#[test]
fn protocol_error_codes_reject_non_literals() {
    for value in [
        "",
        "VERSION",
        "session-locked",
        "unknown",
        "auth",
        " version",
        "internal_error ",
    ] {
        assert_eq!(
            value.parse::<ProtocolErrorCode>(),
            Err(ParseProtocolErrorCodeError),
            "unexpectedly accepted protocol error code {value:?}"
        );
    }
}

#[test]
fn protocol_error_code_parse_errors_name_the_input() {
    assert_eq!(ParseProtocolErrorCodeError.to_string(), "unknown protocol error code");
}
