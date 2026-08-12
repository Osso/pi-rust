use pi_coding_agent::cli::{AuthCommandKind, get_auth_command_name, get_auth_command_usage, is_auth_command_help};
use serde_json::{Value, json};

pub(crate) struct Fixture {
    pub(crate) name: &'static str,
    pub(crate) request: Value,
    pub(crate) expected: Value,
}

enum Request {
    Metadata { id: u64, kind: MetadataKind },
    IsHelp { id: u64, args: &'static [&'static str] },
}

#[derive(Clone, Copy)]
enum MetadataKind {
    Check,
    ApiKey,
    BearerToken,
}

impl Request {
    fn id(&self) -> u64 {
        match self {
            Self::Metadata { id, .. } | Self::IsHelp { id, .. } => *id,
        }
    }

    fn into_fixture(self, name: &'static str) -> Fixture {
        let request = self.to_json();
        let expected = self.expected_response();
        Fixture {
            name,
            request,
            expected,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Self::Metadata { id, kind } => json!({
                "id": id,
                "operation": "metadata",
                "kind": kind.wire_name(),
            }),
            Self::IsHelp { id, args } => json!({
                "id": id,
                "operation": "is_help",
                "args": args,
            }),
        }
    }

    fn expected_response(&self) -> Value {
        let output = match self {
            Self::Metadata { kind, .. } => json!({
                "kind": kind.wire_name(),
                "name": get_auth_command_name(kind.rust_kind()),
                "usage": get_auth_command_usage(kind.rust_kind()),
            }),
            Self::IsHelp { args, .. } => {
                let arguments = args.iter().map(|argument| (*argument).to_owned()).collect::<Vec<_>>();
                json!(is_auth_command_help(&arguments))
            }
        };

        json!({
            "id": self.id(),
            "ok": true,
            "output": output,
        })
    }
}

impl MetadataKind {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::ApiKey => "api_key",
            Self::BearerToken => "bearer_token",
        }
    }

    fn rust_kind(self) -> AuthCommandKind {
        match self {
            Self::Check => AuthCommandKind::Check,
            Self::ApiKey => AuthCommandKind::ApiKey,
            Self::BearerToken => AuthCommandKind::BearerToken,
        }
    }
}

pub(crate) fn all() -> Vec<Fixture> {
    [
        metadata_fixtures(),
        help_entry_fixtures(),
        help_flag_fixtures(),
        help_non_match_fixtures(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn metadata_fixtures() -> Vec<Fixture> {
    vec![
        Request::Metadata {
            id: 12000,
            kind: MetadataKind::Check,
        }
        .into_fixture("metadata check"),
        Request::Metadata {
            id: 12001,
            kind: MetadataKind::ApiKey,
        }
        .into_fixture("metadata api key"),
        Request::Metadata {
            id: 12002,
            kind: MetadataKind::BearerToken,
        }
        .into_fixture("metadata bearer token"),
    ]
}

fn help_entry_fixtures() -> Vec<Fixture> {
    vec![
        Request::IsHelp { id: 12003, args: &[] }.into_fixture("empty args"),
        Request::IsHelp {
            id: 12004,
            args: &["status", "--help"],
        }
        .into_fixture("non-auth prefix"),
        Request::IsHelp {
            id: 12005,
            args: &["auth"],
        }
        .into_fixture("auth alone"),
        Request::IsHelp {
            id: 12006,
            args: &["auth", "help", "extra"],
        }
        .into_fixture("auth help extra"),
    ]
}

fn help_flag_fixtures() -> Vec<Fixture> {
    vec![
        Request::IsHelp {
            id: 12007,
            args: &["auth", "--help"],
        }
        .into_fixture("long help flag"),
        Request::IsHelp {
            id: 12008,
            args: &["auth", "-h"],
        }
        .into_fixture("short help flag"),
        Request::IsHelp {
            id: 12009,
            args: &["auth", "check", "--provider", "example", "--help"],
        }
        .into_fixture("later long help flag"),
        Request::IsHelp {
            id: 12010,
            args: &["auth", "unknown", "duplicate", "-h", "duplicate"],
        }
        .into_fixture("later short help flag with duplicates"),
    ]
}

fn help_non_match_fixtures() -> Vec<Fixture> {
    vec![
        Request::IsHelp {
            id: 12011,
            args: &["auth", "unknown", "extra"],
        }
        .into_fixture("unknown subcommand without help"),
        Request::IsHelp {
            id: 12012,
            args: &["auth", "Help", "--HELP", "-H", "helpful"],
        }
        .into_fixture("case and lookalike flags"),
        Request::IsHelp {
            id: 12013,
            args: &["auth", "héLp", "--help\0", "\0help"],
        }
        .into_fixture("Unicode and NUL lookalikes"),
    ]
}
