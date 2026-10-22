use pi_js_compat::js_json::JsString;
use std::fs;
use std::process::Command as ProcessCommand;

use pi_coding_agent::supervisor::project_resolver::{
    DEFAULT_SUPERVISOR_KB_DIR, SupervisorProjectConfig, SupervisorProjectMapping, extract_remote_repository_identity,
    extract_remote_repository_name, read_remote_repository_names, read_supervisor_project_config,
    resolve_supervisor_project, resolve_supervisor_project_for_cwd,
};
use pi_js_compat::js_json as json;

fn mapping(remotes: &[&str], roots: &[&str]) -> SupervisorProjectMapping {
    SupervisorProjectMapping {
        remotes: (!remotes.is_empty()).then(|| remotes.iter().map(|value| JsString::from(*value)).collect()),
        roots: (!roots.is_empty()).then(|| roots.iter().map(|value| JsString::from(*value)).collect()),
    }
}

fn config(entries: &[(&str, SupervisorProjectMapping)]) -> SupervisorProjectConfig {
    entries
        .iter()
        .map(|(key, value)| (JsString::from(*key), value.clone()))
        .collect()
}

fn sample_config() -> SupervisorProjectConfig {
    config(&[
        (
            "globalcomix",
            mapping(
                &["globalcomix", "globalcomix-api", "globalcomix-web"],
                &["/syncthing/Sync/Projects/globalcomix"],
            ),
        ),
        (
            "mangahelpers",
            mapping(
                &["mangahelpers", "mangahelpers-worker"],
                &["/syncthing/Sync/Projects/mangahelpers"],
            ),
        ),
        (
            "world-of-osso",
            mapping(
                &["world-of-osso", "sentinel", "appfw"],
                &["/syncthing/Sync/Projects/system"],
            ),
        ),
    ])
}

fn resolve(cwd: &str, remotes: &[&str]) -> JsString {
    resolve_supervisor_project(
        &sample_config(),
        &JsString::from(cwd),
        &remotes.iter().map(|value| JsString::from(*value)).collect::<Vec<_>>(),
    )
    .expect("project resolution")
}

#[test]
fn uses_the_configured_project_family_for_a_repository_below_a_metarepo_root() {
    assert_eq!(
        resolve(
            "/syncthing/Sync/Projects/globalcomix/services/api",
            &["unmapped-service"]
        ),
        "globalcomix"
    );
    assert_eq!(resolve("/syncthing/Sync/Projects/globalcomix", &[]), "globalcomix");
}

#[test]
fn does_not_match_a_sibling_directory_sharing_a_root_prefix() {
    assert_eq!(
        resolve("/syncthing/Sync/Projects/globalcomix2", &["globalcomix2"]),
        "globalcomix2"
    );
}

#[test]
fn groups_multiple_repositories_by_configured_remote_names() {
    assert_eq!(
        resolve("/worktrees/manga-worker", &["mangahelpers-worker"]),
        "mangahelpers"
    );
    assert_eq!(resolve("/worktrees/sentinel", &["sentinel"]), "world-of-osso");
    assert_eq!(resolve("/worktrees/sentinel", &["MANGAHELPERS"]), "mangahelpers");
}

#[test]
fn prefers_a_root_match_over_a_remote_match() {
    assert_eq!(
        resolve("/syncthing/Sync/Projects/system/appfw", &["globalcomix-api"]),
        "world-of-osso"
    );
}

#[test]
fn falls_back_from_the_first_remote_name_to_the_directory_basename() {
    let empty = SupervisorProjectConfig::new();
    assert_eq!(
        resolve_supervisor_project(&empty, &JsString::from("/repos/pi"), &["pi-mono".into()])
            .expect("project resolution"),
        "pi-mono"
    );
    assert_eq!(
        resolve_supervisor_project(&empty, &JsString::from("/repos/pi"), &["owner/Pi-Mono".into()])
            .expect("project resolution"),
        "pi-mono"
    );
    assert_eq!(
        resolve_supervisor_project(&empty, &JsString::from("/repos/pi"), &[]).expect("project resolution"),
        "pi"
    );
    assert_eq!(
        resolve_supervisor_project(&empty, &JsString::from("/repos/pi/"), &[]).expect("project resolution"),
        "pi"
    );
    assert_eq!(
        resolve_supervisor_project(&empty, &JsString::from("/"), &[]).expect("project resolution"),
        ""
    );
}

#[test]
fn distinguishes_repositories_with_the_same_basename_by_owner_identity() {
    let ambiguous = config(&[
        ("globalcomix", mapping(&["Globalcomix/ops"], &[])),
        ("mangahelpers", mapping(&["mangahelpers/ops"], &[])),
    ]);
    assert_eq!(
        resolve_supervisor_project(
            &ambiguous,
            &JsString::from("/worktrees/ops"),
            &["MangaHelpers/Ops".into(), "ops".into()]
        )
        .expect("project resolution"),
        "mangahelpers"
    );
}

#[test]
fn honors_javascript_integer_key_ordering() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projects.json");
    fs::write(
        &path,
        r#"{"alpha":{"roots":["/work/alpha"]},"2":{"roots":["/work"]},"1":{"roots":["/work/one"]}}"#,
    )
    .unwrap();
    let ordered = read_supervisor_project_config(&JsString::from(path.to_str().expect("utf-8 test path"))).unwrap();

    assert_eq!(
        ordered.iter().map(|(project_id, _)| project_id).collect::<Vec<_>>(),
        vec!["1", "2", "alpha"],
        "canonical array-index keys come first in ascending order"
    );
    assert_eq!(
        resolve_supervisor_project(&ordered, &JsString::from("/work/project"), &[]).expect("project resolution"),
        "2",
        "integer-key order beats the file order"
    );
    assert_eq!(
        resolve_supervisor_project(&ordered, &JsString::from("/work/one/child"), &[]).expect("project resolution"),
        "1"
    );
}

#[test]
fn extracts_repository_identities_and_names_from_remote_urls() {
    let cases = [
        (
            "git@github.com:earendil-works/pi-mono.git",
            "earendil-works/pi-mono",
            "pi-mono",
        ),
        (
            "https://github.com/earendil-works/pi-mono.git",
            "earendil-works/pi-mono",
            "pi-mono",
        ),
        (
            "ssh://git@github.com/earendil-works/pi-mono",
            "earendil-works/pi-mono",
            "pi-mono",
        ),
        (
            "https://github.com/earendil-works/pi-mono/",
            "earendil-works/pi-mono",
            "pi-mono",
        ),
        ("git@github.com:pi-mono.git", "pi-mono", "pi-mono"),
        ("  https://example.com/a/b/c.git  ", "b/c", "c"),
        ("", "", ""),
    ];
    for (url, identity, name) in cases {
        assert_eq!(
            extract_remote_repository_identity(&JsString::from(url)),
            (!identity.is_empty()).then(|| JsString::from(identity)),
            "{url}"
        );
        assert_eq!(
            extract_remote_repository_name(&JsString::from(url)),
            (!name.is_empty()).then(|| JsString::from(name)),
            "{url}"
        );
    }
    assert_eq!(extract_remote_repository_identity(&JsString::from("///")), None);
}

#[test]
fn reads_and_validates_the_project_config_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projects.json");
    assert_eq!(
        read_supervisor_project_config(&JsString::from(path.to_str().expect("utf-8 test path"))).unwrap(),
        SupervisorProjectConfig::new()
    );

    fs::write(&path, br#"{"a":{"remotes":["r"],"roots":[]},"b":{}}"#).unwrap();
    let parsed = read_supervisor_project_config(&JsString::from(path.to_str().expect("utf-8 test path"))).unwrap();
    assert_eq!(
        parsed.get(&JsString::from("a")),
        Some(&SupervisorProjectMapping {
            remotes: Some(vec!["r".into()]),
            roots: Some(Vec::new()),
        })
    );
    assert_eq!(
        parsed.get(&JsString::from("b")),
        Some(&SupervisorProjectMapping {
            remotes: None,
            roots: None
        })
    );

    for (contents, message) in [
        ("[]", "Supervisor project config must be an object"),
        ("null", "Supervisor project config must be an object"),
        ("{bad", "line 1"),
        (r#"{"a":[]}"#, "Supervisor project mapping must be an object: a"),
        (
            r#"{"a":{"remotes":"r"}}"#,
            "Supervisor project mapping values must be string arrays",
        ),
        (
            r#"{"a":{"roots":["/x",1]}}"#,
            "Supervisor project mapping values must be string arrays",
        ),
    ] {
        fs::write(&path, contents).unwrap();
        let error = read_supervisor_project_config(&JsString::from(path.to_str().expect("utf-8 test path")))
            .expect_err("invalid config must be rejected");
        assert!(error.contains(message), "{contents}: {error:?}");
    }
}

#[test]
fn resolves_from_the_kb_configuration_and_git_remotes() {
    let directory = tempfile::tempdir().unwrap();
    let kb = directory.path().join("kb");
    let repository = directory.path().join("repo");
    fs::create_dir_all(kb.join("memory/supervisor")).unwrap();
    fs::create_dir_all(&repository).unwrap();
    fs::write(
        kb.join("memory/supervisor/projects.json"),
        pi_js_compat::js_json::stringify_lossless(&json!({"globalcomix": {"remotes": ["globalcomix/api"]}})),
    )
    .unwrap();
    run_git(&repository, &["init"]);
    run_git(
        &repository,
        &["remote", "add", "origin", "git@github.com:GlobalComix/api.git"],
    );

    assert_eq!(
        resolve_supervisor_project_for_cwd(
            &JsString::from(repository.to_str().expect("utf-8 test path")),
            &JsString::from(kb.to_str().expect("utf-8 test path"))
        )
        .unwrap(),
        "globalcomix"
    );
    assert_eq!(DEFAULT_SUPERVISOR_KB_DIR, "/syncthing/Sync/KB");
}

#[test]
fn reads_unique_remote_identities_and_names_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let repository = directory.path();
    run_git(repository, &["init"]);
    run_git(
        repository,
        &["remote", "add", "origin", "git@github.com:earendil-works/pi-mono.git"],
    );
    run_git(
        repository,
        &["remote", "add", "upstream", "https://github.com/earendil-works/pi-mono"],
    );
    run_git(
        repository,
        &["remote", "add", "other", "git@github.com:osso/pi-rust.git"],
    );

    assert_eq!(
        read_remote_repository_names(&JsString::from(repository.to_str().expect("utf-8 test path"))),
        vec!["earendil-works/pi-mono", "pi-mono", "osso/pi-rust", "pi-rust"]
    );

    let outside = tempfile::tempdir().unwrap();
    assert!(
        read_remote_repository_names(&JsString::from(outside.path().to_str().expect("utf-8 test path"))).is_empty()
    );
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn malformed_config_reports_the_v8_json_parse_message() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("projects.json");
    fs::write(&path, "{\"a\": \"unterminated").unwrap();

    let error = read_supervisor_project_config(&JsString::from(path.to_str().expect("utf-8 test path")))
        .expect_err("malformed config");

    assert_eq!(error, "Unterminated string in JSON at position 19 (line 1 column 20)");
}
