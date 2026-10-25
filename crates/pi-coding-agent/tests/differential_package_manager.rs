//! Differential tests for `core/package-manager.ts` against the pinned TypeScript module.

use crate::shared_executable_script as executable_script;
#[path = "support/oracle.rs"]
#[allow(dead_code)]
mod oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use oracle::TsOracle;
use pi_coding_agent::core::package_manager::{
    DefaultPackageManager, PackageSettingsSnapshot, PackageSettingsStore, PackageSource,
};
use serde_json::{Value, json};
use tempfile::TempDir;

#[derive(Default)]
struct FakeSettings {
    global: Mutex<PackageSettingsSnapshot>,
    project: Mutex<PackageSettingsSnapshot>,
    trusted: bool,
    npm_command: Option<Vec<JsString>>,
}

impl PackageSettingsStore for FakeSettings {
    fn global_settings(&self) -> PackageSettingsSnapshot {
        self.global.lock().unwrap().clone()
    }

    fn project_settings(&self) -> PackageSettingsSnapshot {
        self.project.lock().unwrap().clone()
    }

    fn set_packages(&self, packages: Vec<PackageSource>) {
        self.global.lock().unwrap().packages = packages;
    }

    fn set_project_packages(&self, packages: Vec<PackageSource>) {
        self.project.lock().unwrap().packages = packages;
    }

    fn is_project_trusted(&self) -> bool {
        self.trusted
    }

    fn get_npm_command(&self) -> Option<Vec<JsString>> {
        self.npm_command.clone()
    }
}

struct Scenario {
    global: Value,
    project: Value,
    trusted: bool,
    npm_command: Option<Vec<JsString>>,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            global: json!({}),
            project: json!({}),
            trusted: false,
            npm_command: None,
        }
    }
}

impl Scenario {
    fn request(&self, operation: &str, cwd: &Path, agent_dir: &Path) -> Value {
        let mut scenario = json!({
            "cwd": cwd.to_string_lossy(),
            "agentDir": agent_dir.to_string_lossy(),
            "global": self.global.clone(),
            "project": self.project.clone(),
            "trusted": self.trusted,
        });
        if let Some(npm_command) = &self.npm_command {
            scenario["global"]["npmCommand"] = json!(npm_command);
        }
        json!({"id": operation, "operation": operation, "scenario": scenario})
    }

    fn manager(&self, cwd: &Path, agent_dir: &Path) -> DefaultPackageManager {
        DefaultPackageManager::new(
            &JsString::from(cwd.to_str().unwrap()),
            &JsString::from(agent_dir.to_str().unwrap()),
            Arc::new(FakeSettings {
                global: Mutex::new(snapshot_from(&self.global)),
                project: Mutex::new(snapshot_from(&self.project)),
                trusted: self.trusted,
                npm_command: self.npm_command.clone(),
            }),
        )
        .unwrap()
    }
}

fn snapshot_from(value: &Value) -> PackageSettingsSnapshot {
    let strings = |key: &str| -> Vec<JsString> {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|entries| entries.iter().filter_map(Value::as_str).map(JsString::from).collect())
            .unwrap_or_default()
    };
    let packages = value
        .get("packages")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| serde_json::from_value(entry.clone()).expect("package source must deserialize"))
                .collect()
        })
        .unwrap_or_default();
    PackageSettingsSnapshot {
        packages,
        extensions: strings("extensions"),
        skills: strings("skills"),
        prompts: strings("prompts"),
        themes: strings("themes"),
    }
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn build_fixture(root: &Path) {
    let agent = root.join("agent");
    let cwd = root.join("project");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    write(&cwd, "extensions/alpha.ts", "a");
    write(&cwd, "extensions/beta.ts", "b");
    write(&cwd, "extensions/nested/index.ts", "nested");
    write(&cwd, "extensions/nested/helper.ts", "helper");
    write(&cwd, "extensions/manifest/index.ts", "ignored");
    write(&cwd, "extensions/manifest/custom.ts", "custom");
    write(
        &cwd,
        "extensions/manifest/package.json",
        r#"{"pi":{"extensions":["custom.ts"]}}"#,
    );
    write(&cwd, ".pi/prompts/keep.md", "keep");
    write(&cwd, ".pi/prompts/skip.md", "skip");
    write(&cwd, ".pi/themes/dark.json", "{}");
    write(&cwd, ".pi/skills/direct/SKILL.md", "---\nname: direct\n---\n");
    write(&cwd, "pkg/extensions/a.ts", "a");
    write(&cwd, "pkg/extensions/b.ts", "b");
    write(&cwd, "pkg/extensions/c.ts", "c");
    write(
        &cwd,
        "pkg/package.json",
        r#"{"name":"pkg","pi":{"extensions":["extensions/*.ts","!extensions/c.ts"]}}"#,
    );
    write(&cwd, "pkg/.gitignore", "extensions/b.ts\n");
    write(
        &cwd,
        "pkg2/package.json",
        r#"{"name":"pkg2","pi":{"skills":["./plugins/*/skills"]}}"#,
    );
    write(
        &cwd,
        "pkg2/plugins/one/skills/alpha-skill/SKILL.md",
        "---\nname: alpha-skill\n---\n",
    );
    write(
        &cwd,
        "pkg2/plugins/two/skills/beta-skill/SKILL.md",
        "---\nname: beta-skill\n---\n",
    );
    write(&agent, "npm/node_modules/my-pkg/extensions/index.ts", "x");
    write(
        &agent,
        "npm/node_modules/my-pkg/package.json",
        r#"{"name":"my-pkg","version":"1.0.0"}"#,
    );
    write(&agent, "prompts/global.md", "global");
    write(&agent, "themes/light.json", "{}");
    write(&agent, "extensions/global.ts", "global");
    write(&agent, "skills/user-skill/SKILL.md", "---\nname: user-skill\n---\n");
    write(
        &cwd,
        "repo/.agents/skills/repo-skill/SKILL.md",
        "---\nname: repo-skill\n---\n",
    );
    write(
        &cwd,
        "repo/packages/app/.codex/skills/codex-skill/SKILL.md",
        "---\nname: codex-skill\n---\n",
    );
    write(
        &cwd,
        "repo/packages/app/.claude/skills/claude-skill/SKILL.md",
        "---\nname: claude-skill\n---\n",
    );
    write(
        &cwd,
        "repo/packages/app/.codex/skills/root.md",
        "---\nname: root\n---\n",
    );
}

fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let temp = TempDir::new().unwrap();
    build_fixture(temp.path());
    let cwd = temp.path().join("project");
    let agent = temp.path().join("agent");
    (temp, cwd, agent)
}

fn oracle_request(oracle: &mut TsOracle, request: Value) -> Value {
    let response = oracle.request(&request);
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    response["output"].clone()
}

fn resolve_scenario(oracle: &mut TsOracle, scenario: &Scenario, cwd: &Path, agent: &Path) -> (Value, Value) {
    let expected = oracle_request(oracle, scenario.request("resolve", cwd, agent));
    let manager = scenario.manager(cwd, agent);
    let resolved = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(manager.resolve(None))
        .unwrap();
    (expected["resolved"].clone(), serde_json::to_value(resolved).unwrap())
}

fn assert_resolve_matches(name: &str, scenario: Scenario, cwd: &Path, agent: &Path) {
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let (expected, actual) = resolve_scenario(&mut oracle, &scenario, cwd, agent);
    assert_eq!(actual, expected, "resolved paths differ for {name}");
}

#[test]
fn differential_parse_source() {
    let (_temp, cwd, agent) = fixture();
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let scenario = Scenario::default();
    let manager = scenario.manager(&cwd, &agent);
    let sources = [
        "npm:left-pad",
        "npm:left-pad@1.2.3",
        "npm:@scope/pkg",
        "npm:@scope/pkg@^1.2.3",
        "npm:example@latest",
        "npm:example?",
        "npm:pkg@",
        "npm:pkg@ ",
        "npm:pkg@*",
        "npm:pkg@1",
        "npm:pkg@1.2",
        "npm:pkg@~1.2.3",
        "npm:pkg@^0.0.3",
        "npm:pkg@>=1.2.3 <2.0.0",
        "npm:pkg@1.2.3 - 2.0.0",
        "npm:pkg@^1.0.0 || ^2.0.0",
        "https://github.com/user/repo",
        "https://github.com/user/repo.git",
        "ssh://git@github.com/user/repo",
        "git:git@github.com:user/repo",
        "git:git@github.com:user/repo@v1.0.0",
        "git:github.com/user/repo",
        "git@github.com:user/repo",
        "github.com/user/repo",
        "./local/path",
        "../sibling",
        "/absolute/path",
    ];
    for source in sources {
        let mut request = scenario.request("parse_source", &cwd, &agent);
        request["source"] = json!(source);
        let expected = oracle_request(&mut oracle, request);
        let actual = serde_json::to_value(manager.parse_source(&JsString::from(source))).unwrap();
        assert_eq!(actual, expected["parsed"], "parse_source({source})");
    }
}

#[test]
fn differential_npm_ranges_cover_node_normalization_and_invalid_boundaries() {
    let (_temp, cwd, agent) = fixture();
    let scenario = Scenario::default();
    let manager = scenario.manager(&cwd, &agent);
    let mut source = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let mut mismatches = Vec::new();
    for version in [
        "\u{feff}1.2.3",
        "v1.2.3",
        "=1.2.3",
        " V1.2.3 ",
        "01.2.3",
        "1.2.3+build.01",
        "1.2.3-01",
        "1.2.3-abc.01",
        "*",
        "x",
        "X",
        "^0.0",
        "^0.0.x",
        "~>1.2.3",
        ">=1.2.3 || *",
        "1.2.3 || <1.2.3",
        ">1",
        "<1.2",
        "<=1.2.x",
        ">=0.0.0",
        "^1.2.3-beta.0",
        "1.2.3+one - 2.3.4+two",
        "1.2 - 2",
        "\t>= 1.2.3  < 2.0.0 ",
        "1.2.3.4",
        "9007199254740992.0.0",
        "-1.2.3",
        "* - 2",
        "1 - *",
        "x - x",
        "0.0.0 - *",
        ">*",
        "<x",
        "<=*",
        ">=x",
        "1.x.3",
        "*.2.3",
        "1.2.x-beta",
        "1.2+abc",
        ">=1.2.3 >=1.2.3",
        ">=1.2.3 1.2.3",
        "^v1.2.3",
        "~v1.2",
        "~ v1",
        "^",
        "~",
        ">=",
        ">=1.0.0 ||",
        "<0.0.0-0 || >=1",
        "9007199254740991",
        "^9007199254740991.0.0",
        "1.9007199254740991",
        "1.2.9007199254740991",
        ">=1.2.3\u{2003}<2",
        "=v1.2.3",
    ] {
        let spec = format!("npm:probe@{version}");
        let mut request = scenario.request("parse_source", &cwd, &agent);
        request["source"] = json!(spec);
        let expected = oracle_request(&mut source, request);
        let actual = serde_json::to_value(manager.parse_source(&spec.into())).unwrap();
        if actual != expected["parsed"] {
            mismatches.push(json!({"version":version,"actual":actual,"expected":expected["parsed"]}));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}",
        serde_json::to_string_pretty(&mismatches).unwrap()
    );
}

#[test]
fn differential_package_identity() {
    let (_temp, cwd, agent) = fixture();
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let scenario = Scenario::default();
    let manager = scenario.manager(&cwd, &agent);
    for source in [
        "npm:@scope/pkg@1.2.3",
        "https://github.com/user/repo",
        "git:git@github.com:user/repo@v1.0.0",
        "ssh://git@github.com/user/repo",
        "./pkg",
        "pkg",
    ] {
        let mut request = scenario.request("identity", &cwd, &agent);
        request["source"] = json!(source);
        let expected = oracle_request(&mut oracle, request);
        assert_eq!(
            json!(manager.get_package_identity(&JsString::from(source), None).unwrap()),
            expected["identity"],
            "identity({source})"
        );
    }
}

#[test]
fn differential_resolve_empty() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches("empty", Scenario::default(), &cwd, &agent);
}

#[test]
fn differential_resolve_local_and_patterns() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches(
        "local",
        Scenario {
            global: json!({
                "extensions": [
                    "extensions",
                    "!extensions/beta.ts",
                    "-extensions/alpha.ts",
                    "+extensions/beta.ts"
                ]
            }),
            ..Scenario::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
fn differential_resolve_packages_and_filters() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches(
        "packages",
        Scenario {
            global: json!({
                "packages": [
                    "./pkg",
                    "./pkg2",
                    {"source": "npm:my-pkg", "extensions": ["!extensions/index.ts"]}
                ]
            }),
            ..Scenario::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
#[cfg(unix)]
fn differential_glob_decodes_non_utf8_filenames_at_the_node_boundary() {
    use std::os::unix::ffi::OsStringExt;
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("project");
    let agent = temp.path().join("agent");
    let package = cwd.join("pkg");
    fs::create_dir_all(&agent).unwrap();
    write(&package, "valid.ts", "control");
    fs::write(
        package.join(std::ffi::OsString::from_vec(b"bad_\xff.ts".to_vec())),
        "raw bytes",
    )
    .unwrap();
    write(&package, "bad_\u{fffd}.ts", "decoded-name file");
    write(&package, "package.json", r#"{"pi":{"extensions":["*.ts"]}}"#);
    let scenario = Scenario {
        global: json!({"packages":[package]}),
        ..Default::default()
    };
    let mut source = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let (mut expected, mut actual) = resolve_scenario(&mut source, &scenario, &cwd, &agent);
    for output in [&mut expected, &mut actual] {
        output["extensions"]
            .as_array_mut()
            .unwrap()
            .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    }
    assert_eq!(
        expected["extensions"].as_array().unwrap().len(),
        2,
        "must exercise both decoded and control paths"
    );
    assert_eq!(actual, expected);
}

#[test]
fn differential_manifest_globs_follow_explicit_hidden_parent_and_absolute_segments() {
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("project");
    let agent = temp.path().join("agent");
    let package = cwd.join("pkg");
    fs::create_dir_all(&agent).unwrap();
    write(&package, ".hidden/a.ts", "hidden");
    write(&package, "dir/leaf/.secret/b.ts", "secret");
    write(&package, "root.ts", "root");
    write(&package, "dir/top.ts", "top");
    write(&package, "dir/leaf/nested.ts", "nested");
    write(&package, "dir/literal*.ts", "escaped star");
    write(&package, "#tag.ts", "hash");
    write(&package, "dir/!note.ts", "bang");
    write(&cwd, "outside/external.ts", "outside");
    #[cfg(unix)]
    std::os::unix::fs::symlink(cwd.join("outside"), package.join("dir/link")).unwrap();
    let patterns = vec![
        ".hidden/*.ts".to_owned(),
        "../outside/*.ts".to_owned(),
        format!("{}/*.ts", cwd.join("outside").display()),
        "dir/**/../*.ts".to_owned(),
        "**/.secret/*.ts".to_owned(),
        "**/*.ts".to_owned(),
        r"dir/literal\*.ts".to_owned(),
        "dir/{top,top}.ts".to_owned(),
        "dir/{../root,top}.ts".to_owned(),
        "dir/*/".to_owned(),
        "#*.ts".to_owned(),
        "dir/!*.ts".to_owned(),
        "dir/**/*.ts".to_owned(),
        "dir/link/**/*.ts".to_owned(),
    ];
    let scenario = Scenario {
        global: json!({"packages":[package]}),
        ..Default::default()
    };
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    for pattern in patterns {
        write(
            &package,
            "package.json",
            &json!({"pi":{"extensions":[pattern]}}).to_string(),
        );
        let (mut expected, mut actual) = resolve_scenario(&mut oracle, &scenario, &cwd, &agent);
        // One package/precedence group: only filesystem enumeration order is excluded.
        for output in [&mut expected, &mut actual] {
            for kind in ["extensions", "skills", "prompts", "themes"] {
                output[kind]
                    .as_array_mut()
                    .unwrap()
                    .sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
            }
        }
        assert_eq!(actual, expected, "manifest glob {pattern}");
    }
}

#[test]
fn differential_same_scope_package_duplicates_keep_the_first_filter() {
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("project");
    let agent = temp.path().join("agent");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&agent).unwrap();
    write(&cwd, "pkg/extensions/a.ts", "a");
    write(&cwd, "pkg/extensions/b.ts", "b");
    write(
        &cwd,
        "pkg/package.json",
        r#"{"pi":{"extensions":["extensions/a.ts","extensions/b.ts"]}}"#,
    );
    assert_resolve_matches(
        "same-scope first wins",
        Scenario {
            project: json!({"packages":[
                {"source":"../pkg","extensions":["extensions/a.ts"]},
                {"source":"../pkg","extensions":["extensions/b.ts"]}
            ]}),
            trusted: true,
            ..Default::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
fn differential_autoload_property_is_ignored_and_project_storage_owns_the_package() {
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("project");
    let agent = temp.path().join("agent");
    for root in [
        &cwd.join(".pi/npm/node_modules/my-pkg"),
        &agent.join("npm/node_modules/my-pkg"),
    ] {
        write(root, "a.ts", "a");
        write(root, "b.ts", "b");
        write(
            root,
            "package.json",
            r#"{"name":"my-pkg","version":"1.0.0","pi":{"extensions":["a.ts","b.ts"]}}"#,
        );
    }
    assert_resolve_matches(
        "autoload ignored/project owns package",
        Scenario {
            global: json!({"packages":["npm:my-pkg"]}),
            project: json!({"packages":[{"source":"npm:my-pkg","autoload":false,"extensions":["a.ts"]}]}),
            trusted: true,
            ..Default::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
fn differential_resolve_autoload_delta_and_dedupe() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches(
        "delta",
        Scenario {
            global: json!({"packages": ["./pkg"]}),
            project: json!({"packages": [{"source": "./pkg", "autoload": false, "extensions": ["+extensions/c.ts"]}]}),
            trusted: true,
            ..Scenario::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
fn differential_resolve_auto_discovery_and_skills() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches(
        "auto",
        Scenario {
            global: json!({"prompts": ["!prompts/global.md"]}),
            project: json!({"prompts": ["!prompts/skip.md"]}),
            trusted: true,
            ..Scenario::default()
        },
        &cwd,
        &agent,
    );
}

#[test]
fn differential_resolve_untrusted_project() {
    let (_temp, cwd, agent) = fixture();
    assert_resolve_matches("untrusted", Scenario::default(), &cwd, &agent);
}

#[test]
fn differential_resolve_from_nested_project_directory() {
    let (_temp, cwd, agent) = fixture();
    let nested = cwd.join("repo/packages/app");
    assert_resolve_matches(
        "nested",
        Scenario {
            trusted: true,
            ..Scenario::default()
        },
        &nested,
        &agent,
    );
}

#[test]
fn differential_list_configured_packages_and_installed_paths() {
    let (_temp, cwd, agent) = fixture();
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let scenario = Scenario {
        global: json!({"packages": ["npm:my-pkg", "npm:missing", "./pkg"]}),
        project: json!({"packages": [{"source": "npm:project-missing"}]}),
        trusted: true,
        ..Scenario::default()
    };
    let expected = oracle_request(&mut oracle, scenario.request("list_configured_packages", &cwd, &agent));
    let manager = scenario.manager(&cwd, &agent);
    assert_eq!(
        serde_json::to_value(manager.list_configured_packages().unwrap()).unwrap(),
        expected["packages"]
    );

    for (source, scope) in [
        (
            "npm:my-pkg",
            pi_coding_agent::core::package_manager::InstalledScope::User,
        ),
        (
            "npm:missing",
            pi_coding_agent::core::package_manager::InstalledScope::User,
        ),
        ("./pkg", pi_coding_agent::core::package_manager::InstalledScope::User),
    ] {
        let mut request = scenario.request("get_installed_path", &cwd, &agent);
        request["source"] = json!(source);
        request["scope"] = json!(match scope {
            pi_coding_agent::core::package_manager::InstalledScope::User => "user",
            pi_coding_agent::core::package_manager::InstalledScope::Project => "project",
        });
        let expected = oracle_request(&mut oracle, request);
        assert_eq!(
            json!(manager.get_installed_path(&JsString::from(source), scope).unwrap()),
            expected["path"],
            "installed path for {source}"
        );
    }
}

#[test]
fn differential_add_and_remove_sources() {
    let (_temp, cwd, agent) = fixture();
    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let scenario = Scenario {
        global: json!({"packages": ["./pkg", {"source": "npm:left-pad@1.0.0", "extensions": ["!x.ts"]}]}),
        ..Scenario::default()
    };
    for operation in ["add_source_to_settings", "remove_source_from_settings"] {
        for source in ["npm:left-pad@2.0.0", "./pkg/", "npm:unknown", "./pkg"] {
            let mut request = scenario.request(operation, &cwd, &agent);
            request["source"] = json!(source);
            let expected = oracle_request(&mut oracle, request);

            let settings = Arc::new(FakeSettings {
                global: Mutex::new(snapshot_from(&scenario.global)),
                project: Mutex::new(PackageSettingsSnapshot::default()),
                trusted: false,
                npm_command: None,
            });
            let manager = DefaultPackageManager::new(
                &JsString::from(cwd.to_str().unwrap()),
                &JsString::from(agent.to_str().unwrap()),
                settings.clone(),
            )
            .unwrap();
            let changed = if operation == "add_source_to_settings" {
                manager.add_source_to_settings(&JsString::from(source), false).unwrap()
            } else {
                manager
                    .remove_source_from_settings(&JsString::from(source), false)
                    .unwrap()
            };
            let packages = serde_json::to_value(settings.global_settings().packages).unwrap();
            assert_eq!(json!(changed), expected["changed"], "{operation}({source}) changed");
            assert_eq!(
                packages, expected["global"]["packages"],
                "{operation}({source}) packages"
            );
        }
    }
}

fn write_recording_npm(dir: &Path) -> String {
    fs::create_dir_all(dir).unwrap();
    let npm = dir.join("npm");
    executable_script::write_executable_script(
        &npm,
        r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/npm.log"
if [ "$1" = "install" ]; then
  root=""
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "--prefix" ]; then root="$arg"; fi
    prev="$arg"
  done
  mkdir -p "$root/node_modules/fake-pkg"
  printf '{"name":"fake-pkg","version":"1.0.0"}' > "$root/node_modules/fake-pkg/package.json"
fi
exit 0
"#,
    );
    npm.to_string_lossy().into_owned()
}

#[test]
fn differential_install_and_remove_npm_with_fake_binary() {
    let temp = TempDir::new().unwrap();
    let oracle_root = temp.path().join("oracle");
    let rust_root = temp.path().join("rust");
    for root in [&oracle_root, &rust_root] {
        build_fixture(root);
    }
    let oracle_npm = write_recording_npm(&temp.path().join("oracle-bin"));
    let rust_npm = write_recording_npm(&temp.path().join("rust-bin"));

    let oracle_cwd = oracle_root.join("project");
    let oracle_agent = oracle_root.join("agent");
    let rust_cwd = rust_root.join("project");
    let rust_agent = rust_root.join("agent");

    let mut oracle = TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let oracle_scenario = Scenario {
        npm_command: Some(vec![oracle_npm.into()]),
        ..Scenario::default()
    };
    let mut install_request = oracle_scenario.request("install", &oracle_cwd, &oracle_agent);
    install_request["source"] = json!("npm:fake-pkg@1.2.3");
    let expected_install = oracle_request(&mut oracle, install_request);
    let mut remove_request = oracle_scenario.request("remove", &oracle_cwd, &oracle_agent);
    remove_request["source"] = json!("npm:fake-pkg");
    let expected_remove = oracle_request(&mut oracle, remove_request);

    let rust_scenario = Scenario {
        npm_command: Some(vec![rust_npm.into()]),
        ..Scenario::default()
    };
    let manager = rust_scenario.manager(&rust_cwd, &rust_agent);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = events.clone();
    manager.set_progress_callback(Some(Arc::new(move |event| {
        recorded.lock().unwrap().push(event.clone());
        Ok(())
    })));
    runtime
        .block_on(manager.install(&JsString::from("npm:fake-pkg@1.2.3"), false))
        .unwrap();
    let install_progress = events.lock().unwrap().clone();
    let install_event_count = install_progress.len();
    runtime
        .block_on(manager.remove(&JsString::from("npm:fake-pkg"), false))
        .unwrap();
    let all_progress = events.lock().unwrap().clone();

    let normalize = |value: &Value, root: &Path| -> Value {
        let text = value.to_string().replace(&root.to_string_lossy().to_string(), "<root>");
        serde_json::from_str(&text).unwrap()
    };
    assert_eq!(
        normalize(&json!(install_progress), &rust_root),
        normalize(&expected_install["progress"], &oracle_root)
    );
    assert_eq!(
        normalize(&json!(all_progress[install_event_count..]), &rust_root),
        normalize(&expected_remove["progress"], &oracle_root)
    );
    let normalize_log = |text: String, agent: &Path| text.replace(&agent.to_string_lossy().to_string(), "<agent>");
    assert_eq!(
        normalize_log(
            fs::read_to_string(temp.path().join("rust-bin/npm.log")).unwrap(),
            &rust_agent
        ),
        normalize_log(
            fs::read_to_string(temp.path().join("oracle-bin/npm.log")).unwrap(),
            &oracle_agent
        )
    );
    assert!(rust_agent.join("npm/node_modules/fake-pkg/package.json").exists());
    assert!(oracle_agent.join("npm/node_modules/fake-pkg/package.json").exists());
}
