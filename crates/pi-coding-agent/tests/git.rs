use pi_coding_agent::utils::git::{GitSource, GitSourceType, parse_git_url};
use serde_json::json;

fn source(repo: &str, host: &str, path: &str, reference: Option<&str>) -> GitSource {
    GitSource {
        source_type: GitSourceType::Git,
        repo: repo.to_owned(),
        host: host.to_owned(),
        path: path.to_owned(),
        reference: reference.map(str::to_owned),
        pinned: reference.is_some(),
    }
}

#[test]
fn parses_explicit_protocol_urls() {
    let cases = [
        (
            "https://github.com/user/repo",
            source("https://github.com/user/repo", "github.com", "user/repo", None),
        ),
        (
            "https://github.com/user/repo@v1.0.0",
            source(
                "https://github.com/user/repo",
                "github.com",
                "user/repo",
                Some("v1.0.0"),
            ),
        ),
        (
            "https://github.com/user/repo.git",
            source("https://github.com/user/repo.git", "github.com", "user/repo", None),
        ),
        (
            "https://github.com/user/repo/tree/main",
            source(
                "https://github.com/user/repo/tree/main",
                "github.com",
                "user/repo",
                Some("main"),
            ),
        ),
        (
            "https://github.com/user/repo#readme",
            source(
                "https://github.com/user/repo#readme",
                "github.com",
                "user/repo",
                Some("readme"),
            ),
        ),
        (
            "https://gitlab.com/group/sub/repo",
            source(
                "https://gitlab.com/group/sub/repo",
                "gitlab.com",
                "group/sub/repo",
                None,
            ),
        ),
        (
            "https://gitlab.com/group/sub/repo@v2",
            source(
                "https://gitlab.com/group/sub/repo",
                "gitlab.com",
                "group/sub/repo",
                Some("v2"),
            ),
        ),
        (
            "https://bitbucket.org/user/repo",
            source("https://bitbucket.org/user/repo", "bitbucket.org", "user/repo", None),
        ),
        (
            "https://gist.github.com/user/abc123",
            source(
                "https://gist.github.com/user/abc123",
                "gist.github.com",
                "user/abc123",
                None,
            ),
        ),
        (
            "https://git.sr.ht/~user/repo",
            source("https://git.sr.ht/~user/repo", "git.sr.ht", "~user/repo", None),
        ),
        (
            "ssh://git@github.com/user/repo.git",
            source("ssh://git@github.com/user/repo.git", "github.com", "user/repo", None),
        ),
        (
            "https://example.com/user/repo",
            source("https://example.com/user/repo", "example.com", "user/repo", None),
        ),
        (
            "https://example.com/user/repo@v1",
            source("https://example.com/user/repo", "example.com", "user/repo", Some("v1")),
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(parse_git_url(input), Some(expected), "{input}");
    }
}

#[test]
fn parses_git_prefixed_shorthands() {
    let cases = [
        (
            "git:git@github.com:user/repo",
            source("git@github.com:user/repo", "github.com", "user/repo", None),
        ),
        (
            "git:github.com/user/repo",
            source("https://github.com/user/repo", "github.com", "user/repo", None),
        ),
        (
            "git:git@github.com:user/repo@v1.0.0",
            source("git@github.com:user/repo", "github.com", "user/repo", Some("v1.0.0")),
        ),
        (
            "git:user/repo",
            source("https://user/repo", "github.com", "user/repo", None),
        ),
        (
            "git:github.com/user/repo@v1",
            source("https://github.com/user/repo", "github.com", "user/repo", Some("v1")),
        ),
        (
            "git:example.com/user/repo",
            source("https://example.com/user/repo", "example.com", "user/repo", None),
        ),
        (
            "git:localhost/user/repo",
            source("https://localhost/user/repo", "localhost", "user/repo", None),
        ),
        (
            "git:github.com/user/repo#branch",
            source(
                "https://github.com/user/repo#branch",
                "github.com",
                "user/repo",
                Some("branch"),
            ),
        ),
        (
            "git:github.com/user/repo@feature/x",
            source(
                "https://github.com/user/repo",
                "github.com",
                "user/repo",
                Some("feature/x"),
            ),
        ),
        (
            "git:git@github.com:user/repo@feature/x",
            source("git@github.com:user/repo", "github.com", "user/repo", Some("feature/x")),
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(parse_git_url(input), Some(expected), "{input}");
    }
}

#[test]
fn rejects_unsupported_shorthands_without_git_prefix() {
    for input in ["git@github.com:user/repo", "github.com/user/repo", "user/repo"] {
        assert_eq!(parse_git_url(input), None, "{input}");
    }
}

#[test]
fn rejects_unsafe_git_install_paths() {
    for input in [
        "git:git@evil.example:../../victim/repo",
        "https://evil.example/..%2F..%2Fvictim/repo",
        "https://evil.example/..%2F..%2Fvictim/repo%",
        "git:git@evil.example:/absolute/repo",
        "git:git@evil.example:user\\repo/name",
        "git:git@evil.example:user/repo\0name",
    ] {
        assert_eq!(parse_git_url(input), None, "{input}");
    }
}

#[test]
fn rejects_incomplete_repositories() {
    for input in [
        "https://github.com/user",
        "https://github.com/",
        "git:example/user/repo",
    ] {
        assert_eq!(parse_git_url(input), None, "{input}");
    }
}

#[test]
fn preserves_source_bug_for_git_protocol_repo_url() {
    // Discrepancy: the pinned source emits four slashes for a `git://` URL
    // because the generic parser reparses it with the https prefix.
    assert_eq!(
        parse_git_url("git://github.com/user/repo"),
        Some(source(
            "https:////github.com/user/repo",
            "github.com",
            "user/repo",
            None
        ))
    );
}

#[test]
fn preserves_hash_and_ref_interaction_from_source() {
    assert_eq!(
        parse_git_url("https://github.com/user/repo@main#x"),
        Some(source(
            "https://github.com/user/repo#x",
            "github.com",
            "user/repo",
            Some("x#main")
        ))
    );
    assert_eq!(
        parse_git_url("https://github.com/user/repo@"),
        Some(source(
            "https://github.com/user/repo@",
            "github.com",
            "user/repo@",
            None
        ))
    );
}

#[test]
fn serializes_with_source_wire_names() {
    let value = serde_json::to_value(source(
        "https://github.com/user/repo",
        "github.com",
        "user/repo",
        Some("main"),
    ))
    .expect("GitSource must serialize");
    assert_eq!(
        value,
        json!({
            "type": "git",
            "repo": "https://github.com/user/repo",
            "host": "github.com",
            "path": "user/repo",
            "ref": "main",
            "pinned": true
        })
    );
}
