#[path = "../examples/extensions/subagent/agents.rs"]
mod agents;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::{JsString, JsValue};
use serde_json::json;
use std::fs;

#[test]
fn discovery_and_listing_match_pinned_module_on_private_files() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("user");
    let project = root.path().join("project");
    let config = pi_coding_agent::config::config_dir_name();
    let local = project.join(config.to_utf8().unwrap()).join("agents");
    fs::create_dir_all(user.join("agents")).unwrap();
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(project.join("nested/deep")).unwrap();
    for (dir, name, text) in [
        (
            user.join("agents"),
            "a.md",
            "---\nname: shared\ndescription: user first\ntools: 'read, ,bash '\nmodel: faux/m\n---\nUser body",
        ),
        (
            user.join("agents"),
            "b.md",
            "---\nname: only-user\ndescription: user second\n---\nBody",
        ),
        (
            local.clone(),
            "a.md",
            "---\nname: shared\ndescription: project wins\ntools: ', ,'\n---\nProject body",
        ),
        (
            local.clone(),
            "b.md",
            "---\nname: local🚀\ndescription: local second\n---\nLocal body",
        ),
        (
            local.clone(),
            "ignored.md",
            "---\nname: missing-description\n---\nIgnored",
        ),
        (
            local.clone(),
            "ignored.MD",
            "---\nname: wrong-case\ndescription: ignored\n---\nIgnored",
        ),
    ] {
        fs::write(dir.join(name), text).unwrap();
    }
    std::os::unix::fs::symlink(local.join("b.md"), local.join("c.md")).unwrap();
    std::os::unix::fs::symlink(local.join("absent"), local.join("broken.md")).unwrap();
    fs::create_dir(local.join("directory.md")).unwrap();
    let cwd = project.join("nested/deep");
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "example-subagent-agents-oracle.ts",
        "packages/coding-agent/examples/extensions/subagent/agents.ts",
        &[("PI_CODING_AGENT_DIR", user.clone().into_os_string())],
    );
    for scope in ["user", "project", "both"] {
        let actual = agents::discover_agents(&cwd, &JsString::from(scope), &user).unwrap();
        let expected = ts.request(&json!({"cwd":cwd,"scope":scope,"limits":[0,1,8,-1]}));
        assert_eq!(serde_json::to_value(&actual).unwrap(), expected["discovery"]);
        for (index, limit) in [0, 1, 8, -1].into_iter().enumerate() {
            assert_eq!(
                agents::format_agent_list(&actual.agents, limit),
                JsValue::from(expected["lists"][index].clone())
            );
        }
        if scope == "both" {
            assert_eq!(
                actual.agents.iter().map(|a| &a.name).collect::<Vec<_>>(),
                ["shared", "only-user", "local🚀"]
            );
        }
    }
    fs::write(local.join("z.md"), "---\nname: [\n---\nBroken").unwrap();
    assert!(agents::discover_agents(&cwd, &JsString::from("project"), &user).is_err());
    assert_eq!(
        ts.request(&json!({"cwd":cwd,"scope":"project","limits":[]}))["failed"],
        true
    );
    fs::remove_dir_all(&local).unwrap();
    let empty = agents::discover_agents(root.path(), &JsString::from("project"), &user).unwrap();
    assert!(empty.agents.is_empty());
    assert_eq!(
        agents::format_agent_list(&empty.agents, 5),
        JsValue::from(json!({"text":"none","remaining":0}))
    );
    assert!(empty.project_agents_dir.is_none());
}
