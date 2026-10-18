use pi_coding_agent::cli::experimental::transport_address::{TransportAddress, TransportKind, parse_transport_address};

fn parse(value: &str) -> Result<TransportAddress, String> {
    parse_transport_address(value, "--listen")
}

fn error(value: &str, option: &str) -> String {
    parse_transport_address(value, option).expect_err("address must be rejected")
}

#[test]
fn accepts_absolute_unix_paths() {
    assert_eq!(
        parse("unix:///tmp/pi.sock"),
        Ok(TransportAddress {
            transport: TransportKind::Unix,
            path: "/tmp/pi.sock".into(),
        })
    );
    assert_eq!(
        serde_json::to_value(parse("unix:///tmp/pi.sock").unwrap()).unwrap(),
        serde_json::json!({"transport": "unix", "path": "/tmp/pi.sock"})
    );
    assert_eq!(parse("unix:///").unwrap().path, "/");
    assert_eq!(parse("unix:///tmp/pi.sock/").unwrap().path, "/tmp/pi.sock/");
}

#[test]
fn decodes_percent_escapes_in_the_path() {
    assert_eq!(parse("unix:///tmp/a%20b").unwrap().path, "/tmp/a b");
    assert_eq!(parse("unix:///tmp/a%2Fb").unwrap().path, "/tmp/a/b");
    assert_eq!(parse("unix:///tmp/%C3%BC").unwrap().path, "/tmp/ü");
}

#[test]
fn rejects_unparseable_addresses() {
    assert_eq!(
        error("/tmp/pi.sock", "--listen"),
        "Invalid --listen address \"/tmp/pi.sock\""
    );
    assert_eq!(error("", "--listen"), "Invalid --listen address \"\"");
    assert_eq!(
        error("unix:/tmp/pi.sock", "--connect"),
        "Invalid --connect address \"unix:/tmp/pi.sock\""
    );
}

#[test]
fn rejects_unsupported_transports() {
    assert_eq!(
        error("ws://localhost:8080", "--listen"),
        "Unsupported --listen transport \"ws:\""
    );
    assert_eq!(
        error("http://localhost/pi.sock", "--connect"),
        "Unsupported --connect transport \"http:\""
    );
}

#[test]
fn rejects_authorities_credentials_and_ports() {
    for value in [
        "unix://relative.sock",
        "unix://host/tmp/pi.sock",
        "unix://user@host/tmp/x",
        "unix://user:secret@host/tmp/x",
        "unix://host:1234/tmp/x",
    ] {
        assert_eq!(
            error(value, "--listen"),
            "Unix transport address must not include an authority",
            "{value}"
        );
    }
}

#[test]
fn rejects_query_fragments_and_non_canonical_forms() {
    for value in [
        "unix:///tmp/pi.sock?wrong=value",
        "unix:///tmp/pi.sock#fragment",
        "unix:////tmp/pi.sock",
        "unix:///tmp/pi sock",
        "UNIX:///tmp/pi.sock",
        "unix:///tmp/../etc/passwd",
    ] {
        assert_eq!(
            error(value, "--listen"),
            format!("Invalid --listen address \"{value}\""),
            "{value}"
        );
    }
}

#[test]
fn rejects_nul_and_malformed_percent_escapes() {
    for value in ["unix:///tmp/%00pi.sock", "unix:///tmp/%zz", "unix:///tmp/%2"] {
        assert_eq!(
            error(value, "--listen"),
            format!("Invalid --listen address \"{value}\""),
            "{value}"
        );
    }
}

#[test]
fn rejects_percent_encoded_dot_segments() {
    // WHATWG path normalization removes `%2E%2E`, so the canonical href no longer
    // matches the input; the decoded pathname always starts with `/`, which makes
    // the source's absolute-path check unreachable through this grammar.
    assert_eq!(
        error("unix:///tmp/%2E%2E/pi.sock", "--listen"),
        "Invalid --listen address \"unix:///tmp/%2E%2E/pi.sock\""
    );
}
