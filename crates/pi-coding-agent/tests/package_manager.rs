//! Behavioral tests for `core/package-manager.ts`.

#[path = "support/executable_script.rs"]
mod executable_script;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::package_manager::{
    DefaultPackageManager, PackageFilter, PackageSettingsSnapshot, PackageSettingsStore, PackageSource, ParsedSource,
    SettingsManagerStore, SourceScope,
};
use serde_json::json;
use tempfile::TempDir;

#[derive(Default)]
struct FakeSettings {
    global: Mutex<PackageSettingsSnapshot>,
    project: Mutex<PackageSettingsSnapshot>,
    trusted: AtomicBool,
    npm_command: Mutex<Option<Vec<JsString>>>,
}

impl FakeSettings {
    fn new(global: PackageSettingsSnapshot, project: PackageSettingsSnapshot, trusted: bool) -> Self {
        Self {
            global: Mutex::new(global),
            project: Mutex::new(project),
            trusted: AtomicBool::new(trusted),
            npm_command: Mutex::new(None),
        }
    }

    fn with_npm_command(self, command: Vec<JsString>) -> Self {
        *self.npm_command.lock().unwrap() = Some(command);
        self
    }
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
        self.trusted.load(Ordering::SeqCst)
    }

    fn get_npm_command(&self) -> Option<Vec<JsString>> {
        self.npm_command.lock().unwrap().clone()
    }
}

fn snapshot(packages: Vec<PackageSource>) -> PackageSettingsSnapshot {
    PackageSettingsSnapshot {
        packages,
        ..PackageSettingsSnapshot::default()
    }
}

struct Fixture {
    temp: TempDir,
    agent_dir: PathBuf,
    cwd: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let agent_dir = temp.path().join("agent");
        let cwd = temp.path().join("project");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        Self { temp, agent_dir, cwd }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.cwd.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn write_agent(&self, relative: &str, contents: &str) {
        let path = self.agent_dir.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn manager(&self, settings: FakeSettings) -> DefaultPackageManager {
        DefaultPackageManager::new(
            &JsString::from(self.cwd.to_str().unwrap()),
            &JsString::from(self.agent_dir.to_str().unwrap()),
            Arc::new(settings),
        )
        .unwrap()
    }

    fn manager_with(&self, settings: Arc<dyn PackageSettingsStore>) -> DefaultPackageManager {
        DefaultPackageManager::new(
            &JsString::from(self.cwd.to_str().unwrap()),
            &JsString::from(self.agent_dir.to_str().unwrap()),
            settings,
        )
        .unwrap()
    }

    fn path(&self, relative: &str) -> String {
        self.cwd.join(relative).to_string_lossy().into_owned()
    }

    fn agent_path(&self, relative: &str) -> String {
        self.agent_dir.join(relative).to_string_lossy().into_owned()
    }
}

fn ends_with(path: &JsString, suffix: &str) -> bool {
    path.replace('\\', "/").ends_with(suffix)
}

fn enabled_paths(resources: &[pi_coding_agent::core::package_manager::ResolvedResource]) -> Vec<JsString> {
    resources
        .iter()
        .filter(|resource| resource.enabled)
        .map(|resource| resource.path.clone())
        .collect()
}

fn write_fake_npm(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    fs::create_dir_all(dir).unwrap();
    executable_script::write_executable_script(&path, body);
    path.to_string_lossy().into_owned()
}

#[test]
fn parses_npm_sources() {
    let fixture = Fixture::new();
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));
    let cases = [
        ("npm:left-pad", "left-pad", None, false),
        ("npm:left-pad@1.2.3", "left-pad", Some("1.2.3"), true),
        ("npm:@scope/pkg", "@scope/pkg", None, false),
        ("npm:@scope/pkg@^1.2.3", "@scope/pkg", Some("^1.2.3"), false),
        ("npm:example@latest", "example", Some("latest"), false),
    ];
    for (source, name, version, pinned) in cases {
        let ParsedSource::Npm(parsed) = manager.parse_source(&JsString::from(source)) else {
            panic!("{source} must parse as npm");
        };
        assert_eq!(parsed.name, name, "{source}");
        assert_eq!(parsed.version, version.map(JsString::from), "{source}");
        assert_eq!(parsed.pinned, pinned, "{source}");
        assert_eq!(parsed.spec, source.trim_start_matches("npm:"), "{source}");
    }
}

#[test]
fn parses_git_and_local_sources() {
    let fixture = Fixture::new();
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));

    let ParsedSource::Git(git) = manager.parse_source(&JsString::from("https://github.com/user/repo")) else {
        panic!("https URL must parse as git");
    };
    assert_eq!(git.host, "github.com");
    assert_eq!(git.path, "user/repo");
    assert!(!git.pinned);

    let ParsedSource::Git(git) = manager.parse_source(&JsString::from("git:git@github.com:user/repo@v1.0.0")) else {
        panic!("scp shorthand must parse as git");
    };
    assert_eq!(git.repo, "git@github.com:user/repo");
    assert_eq!(git.reference.as_ref(), Some(&JsString::from("v1.0.0")));
    assert!(git.pinned);

    for source in [
        "git@github.com:user/repo",
        "github.com/user/repo",
        "./local/path",
        "../sibling",
    ] {
        assert!(
            matches!(manager.parse_source(&JsString::from(source)), ParsedSource::Local(_)),
            "{source} must parse as local"
        );
    }
}

#[test]
fn resolves_no_sources_to_auto_discovered_only() {
    let fixture = Fixture::new();
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));

    let resolved = futures_block(manager.resolve(None)).unwrap();
    assert!(resolved.extensions.is_empty());
    assert!(resolved.prompts.is_empty());
    assert!(resolved.themes.is_empty());
}

#[test]
fn resolves_local_extension_files_and_directories() {
    let fixture = Fixture::new();
    fixture.write_agent("extensions/single.ts", "export default 1;");
    fixture.write_agent("extensions/dir/index.ts", "export default 2;");
    let settings = FakeSettings::new(
        PackageSettingsSnapshot {
            extensions: vec![
                "extensions/single.ts".to_owned().into(),
                "extensions/dir".to_owned().into(),
            ],
            ..PackageSettingsSnapshot::default()
        },
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let paths = enabled_paths(&resolved.extensions);
    assert!(paths.iter().any(|path| ends_with(path, "extensions/single.ts")));
    assert!(paths.iter().any(|path| ends_with(path, "extensions/dir/index.ts")));
    assert_eq!(resolved.extensions[0].metadata.source, "local");
    assert_eq!(resolved.extensions[0].metadata.scope, SourceScope::User);
    assert_eq!(
        resolved.extensions[0].metadata.origin,
        pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel
    );
}

#[test]
fn applies_include_exclude_and_force_patterns_to_top_level_entries() {
    let fixture = Fixture::new();
    for name in ["alpha.ts", "beta.ts", "foo.ts"] {
        fixture.write_agent(&format!("extensions/{name}"), "x");
    }
    let settings = FakeSettings::new(
        PackageSettingsSnapshot {
            extensions: vec![
                "extensions".to_owned().into(),
                "!extensions/beta.ts".to_owned().into(),
                JsString::from("-extensions/foo.ts"),
            ],
            ..PackageSettingsSnapshot::default()
        },
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let disabled: Vec<JsString> = resolved
        .extensions
        .iter()
        .filter(|resource| !resource.enabled)
        .map(|resource| resource.path.clone())
        .collect();
    assert_eq!(disabled.len(), 2);
    assert!(disabled.iter().any(|path| ends_with(path, "beta.ts")));
    assert!(disabled.iter().any(|path| ends_with(path, "foo.ts")));
    assert!(
        enabled_paths(&resolved.extensions)
            .iter()
            .any(|path| ends_with(path, "alpha.ts"))
    );
}

#[test]
fn force_include_restores_an_excluded_file() {
    let fixture = Fixture::new();
    for name in ["alpha.ts", "beta.ts"] {
        fixture.write_agent(&format!("extensions/{name}"), "x");
    }
    let settings = FakeSettings::new(
        PackageSettingsSnapshot {
            extensions: vec![
                "extensions".to_owned().into(),
                "!extensions/*.ts".to_owned().into(),
                JsString::from("+extensions/beta.ts"),
            ],
            ..PackageSettingsSnapshot::default()
        },
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let enabled = enabled_paths(&resolved.extensions);
    assert_eq!(enabled.len(), 1);
    assert!(ends_with(&enabled[0], "beta.ts"));
}

#[test]
fn manifest_globs_and_overrides_control_package_resources() {
    let fixture = Fixture::new();
    let package_dir = fixture.agent_dir.join("pkg");
    fs::create_dir_all(package_dir.join("extensions")).unwrap();
    for name in ["a.ts", "b.ts"] {
        fs::write(package_dir.join("extensions").join(name), "x").unwrap();
    }
    fs::write(
        package_dir.join("package.json"),
        json!({"name": "pkg", "pi": {"extensions": ["extensions/*.ts", "!extensions/b.ts"]}}).to_string(),
    )
    .unwrap();

    let settings = FakeSettings::new(
        snapshot(vec![PackageSource::Name("./pkg".to_owned().into())]),
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let enabled = enabled_paths(&resolved.extensions);
    assert_eq!(enabled.len(), 1, "{enabled:?}");
    assert!(ends_with(&enabled[0], "extensions/a.ts"));
}

#[test]
fn package_filters_apply_on_top_of_manifest_filters() {
    let fixture = Fixture::new();
    let package_dir = fixture.agent_dir.join("filtered");
    fs::create_dir_all(package_dir.join("extensions")).unwrap();
    for name in ["a.ts", "b.ts", "c.ts"] {
        fs::write(package_dir.join("extensions").join(name), "x").unwrap();
    }
    fs::write(
        package_dir.join("package.json"),
        json!({"name": "filtered", "pi": {"extensions": ["extensions/*.ts", "!extensions/c.ts"]}}).to_string(),
    )
    .unwrap();

    let filter = PackageFilter {
        source: "./filtered".to_owned().into(),
        extensions: Some(vec!["!extensions/b.ts".to_owned().into()]),
        ..PackageFilter::default()
    };
    let settings = FakeSettings::new(snapshot(vec![PackageSource::Filter(filter)]), Default::default(), false);
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let enabled = enabled_paths(&resolved.extensions);
    assert_eq!(enabled.len(), 1, "{enabled:?}");
    assert!(ends_with(&enabled[0], "extensions/a.ts"));
}

#[test]
fn dedupes_project_and_global_packages_with_project_winning() {
    let fixture = Fixture::new();
    let package_dir = fixture.temp.path().join("dup");
    fs::create_dir_all(package_dir.join("extensions")).unwrap();
    fs::write(package_dir.join("extensions/one.ts"), "x").unwrap();
    let shared = package_dir.to_string_lossy().into_owned();
    let settings = FakeSettings::new(
        snapshot(vec![PackageSource::Name(shared.clone().into())]),
        snapshot(vec![PackageSource::Name(shared.into())]),
        true,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let package_resources: Vec<_> = resolved
        .extensions
        .iter()
        .filter(|resource| resource.metadata.origin == pi_coding_agent::core::package_manager::MetadataOrigin::Package)
        .collect();
    assert_eq!(package_resources.len(), 1);
    assert_eq!(package_resources[0].metadata.scope, SourceScope::Project);
}

#[test]
fn multi_file_extensions_only_load_index_or_manifest_entries() {
    let fixture = Fixture::new();
    fixture.write_agent("extensions/main.ts", "main");
    fixture.write_agent("extensions/helper.ts", "helper");
    fixture.write_agent("extensions/nested/index.ts", "nested");
    fixture.write_agent("extensions/nested/helper.ts", "nested-helper");
    fixture.write_agent("extensions/empty/readme.md", "no entry");
    fixture.write_agent("extensions/manifest/index.ts", "ignored when manifest exists");
    fixture.write_agent("extensions/manifest/custom.ts", "custom");
    fixture.write_agent(
        "extensions/manifest/package.json",
        &json!({"pi": {"extensions": ["custom.ts"]}}).to_string(),
    );
    let settings = FakeSettings::new(
        PackageSettingsSnapshot {
            extensions: vec!["extensions".to_owned().into()],
            ..PackageSettingsSnapshot::default()
        },
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let enabled = enabled_paths(&resolved.extensions);
    assert!(enabled.iter().any(|path| ends_with(path, "extensions/main.ts")));
    assert!(enabled.iter().any(|path| ends_with(path, "extensions/nested/index.ts")));
    assert!(
        enabled
            .iter()
            .any(|path| ends_with(path, "extensions/manifest/custom.ts"))
    );
    assert!(enabled.iter().any(|path| ends_with(path, "extensions/helper.ts")));
    assert!(!enabled.iter().any(|path| ends_with(path, "nested/helper.ts")));
    assert!(!enabled.iter().any(|path| ends_with(path, "empty/readme.md")));
}

#[test]
fn auto_discovers_project_and_user_resources_with_overrides() {
    let fixture = Fixture::new();
    fixture.write(".pi/prompts/keep.md", "keep");
    fixture.write(".pi/prompts/skip.md", "skip");
    fixture.write(".pi/themes/dark.json", "{}");
    fixture.write(".pi/skills/direct/SKILL.md", "---\nname: direct\n---\n");
    fixture.write_agent("prompts/global.md", "global");
    fixture.write_agent("themes/light.json", "{}");
    fixture.write_agent("extensions/global.ts", "global");
    let settings = FakeSettings::new(
        PackageSettingsSnapshot {
            prompts: vec!["!prompts/skip.md".to_owned().into()],
            ..PackageSettingsSnapshot::default()
        },
        PackageSettingsSnapshot {
            prompts: vec!["!prompts/skip.md".to_owned().into()],
            ..PackageSettingsSnapshot::default()
        },
        true,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    let prompts = &resolved.prompts;
    assert!(
        prompts
            .iter()
            .any(|resource| ends_with(&resource.path, "keep.md") && resource.enabled)
    );
    assert!(
        prompts
            .iter()
            .any(|resource| ends_with(&resource.path, "skip.md") && !resource.enabled)
    );
    assert!(
        prompts
            .iter()
            .any(|resource| ends_with(&resource.path, "global.md") && resource.enabled)
    );
    assert!(
        resolved
            .themes
            .iter()
            .any(|resource| ends_with(&resource.path, "dark.json"))
    );
    assert!(
        resolved
            .themes
            .iter()
            .any(|resource| ends_with(&resource.path, "light.json"))
    );
    assert!(
        resolved
            .skills
            .iter()
            .any(|resource| ends_with(&resource.path, "direct/SKILL.md"))
    );
    assert!(
        resolved
            .extensions
            .iter()
            .any(|resource| ends_with(&resource.path, "global.ts"))
    );
    assert!(
        resolved
            .prompts
            .iter()
            .all(|resource| resource.metadata.source == "auto")
    );
}

#[test]
fn untrusted_project_skips_project_auto_discovery() {
    let fixture = Fixture::new();
    fixture.write(".pi/prompts/project.md", "project");
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));

    let resolved = futures_block(manager.resolve(None)).unwrap();
    assert!(
        !resolved
            .prompts
            .iter()
            .any(|resource| ends_with(&resource.path, "project.md"))
    );
}

#[test]
fn discovers_ancestor_agents_and_tool_skill_directories() {
    let fixture = Fixture::new();
    let repo = fixture.cwd.join("repo");
    let nested = repo.join("packages/app");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(repo.join(".agents/skills/repo-skill")).unwrap();
    fs::write(
        repo.join(".agents/skills/repo-skill/SKILL.md"),
        "---\nname: repo-skill\n---\n",
    )
    .unwrap();
    fs::create_dir_all(nested.join(".codex/skills/codex-skill")).unwrap();
    fs::write(
        nested.join(".codex/skills/codex-skill/SKILL.md"),
        "---\nname: codex-skill\n---\n",
    )
    .unwrap();
    fs::create_dir_all(nested.join(".claude/skills/claude-skill")).unwrap();
    fs::write(
        nested.join(".claude/skills/claude-skill/SKILL.md"),
        "---\nname: claude-skill\n---\n",
    )
    .unwrap();
    fs::write(nested.join(".codex/skills/root.md"), "---\nname: root\n---\n").unwrap();

    let settings = FakeSettings::new(Default::default(), Default::default(), true);
    let manager = DefaultPackageManager::new(
        &JsString::from(nested.to_str().unwrap()),
        &JsString::from(fixture.agent_dir.to_str().unwrap()),
        Arc::new(settings),
    )
    .unwrap();
    let resolved = futures_block(manager.resolve(None)).unwrap();
    let skills = enabled_paths(&resolved.skills);
    assert!(skills.iter().any(|path| ends_with(path, "repo-skill/SKILL.md")));
    assert!(skills.iter().any(|path| ends_with(path, "codex-skill/SKILL.md")));
    assert!(skills.iter().any(|path| ends_with(path, "claude-skill/SKILL.md")));
    assert!(skills.iter().any(|path| ends_with(path, "skills/root.md")));
}

#[test]
fn gitignore_files_filter_package_resources_but_not_pi_auto_discovery() {
    let fixture = Fixture::new();
    let package_dir = fixture.agent_dir.join("pkg");
    fs::create_dir_all(package_dir.join("extensions")).unwrap();
    fs::write(package_dir.join("extensions/keep.ts"), "k").unwrap();
    fs::write(package_dir.join("extensions/skip.ts"), "s").unwrap();
    fs::write(package_dir.join("extensions/.gitignore"), "skip.ts\n").unwrap();
    fixture.write(".pi/.gitignore", "prompts/*.md\n");
    fixture.write(".pi/prompts/kept.md", "kept");

    let settings = FakeSettings::new(
        snapshot(vec![PackageSource::Name("./pkg".to_owned().into())]),
        Default::default(),
        true,
    );
    let manager = fixture.manager(settings);
    let resolved = futures_block(manager.resolve(None)).unwrap();
    let package_paths: Vec<JsString> = resolved
        .extensions
        .iter()
        .filter(|resource| resource.metadata.origin == pi_coding_agent::core::package_manager::MetadataOrigin::Package)
        .map(|resource| resource.path.clone())
        .collect();
    assert!(package_paths.iter().any(|path| ends_with(path, "keep.ts")));
    assert!(!package_paths.iter().any(|path| ends_with(path, "skip.ts")));
    assert!(
        resolved
            .prompts
            .iter()
            .any(|resource| ends_with(&resource.path, "prompts/kept.md"))
    );
}

#[test]
fn installed_npm_packages_resolve_from_the_managed_root() {
    let fixture = Fixture::new();
    fixture.write_agent("npm/node_modules/my-pkg/extensions/index.ts", "x");
    fixture.write_agent(
        "npm/node_modules/my-pkg/package.json",
        &json!({"name": "my-pkg", "version": "1.0.0"}).to_string(),
    );
    let settings = FakeSettings::new(
        snapshot(vec![PackageSource::Name("npm:my-pkg".to_owned().into())]),
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let resolved = futures_block(manager.resolve(None)).unwrap();
    assert!(
        resolved
            .extensions
            .iter()
            .any(|resource| ends_with(&resource.path, "my-pkg/extensions/index.ts"))
    );
    assert_eq!(
        manager
            .get_installed_path(
                &JsString::from("npm:my-pkg"),
                pi_coding_agent::core::package_manager::InstalledScope::User
            )
            .unwrap(),
        Some(JsString::from(fixture.agent_path("npm/node_modules/my-pkg")))
    );
}

#[test]
fn installs_npm_packages_with_the_managed_argv_and_reports_progress() {
    let fixture = Fixture::new();
    let log = fixture.temp.path().join("npm.log");
    let npm = write_fake_npm(
        &fixture.temp.path().join("bin"),
        "npm",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
if [ "$1" = "install" ]; then
  spec="$2"
  name="${{spec%@*}}"
  root=""
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "--prefix" ]; then root="$arg"; fi
    prev="$arg"
  done
  mkdir -p "$root/node_modules/$name"
  printf '{{"name":"%s","version":"1.0.0"}}' "$name" > "$root/node_modules/$name/package.json"
  printf '{{"name":"pi-extensions","private":true}}' > "$root/package.json"
fi
exit 0
"#,
            log.display()
        ),
    );
    let settings = Arc::new(
        FakeSettings::new(Default::default(), Default::default(), false).with_npm_command(vec![npm.clone().into()]),
    );
    let manager = fixture.manager_with(settings);
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = events.clone();
    manager.set_progress_callback(Some(Arc::new(move |event| {
        recorded.lock().unwrap().push(event.clone());
        Ok(())
    })));

    futures_block(manager.install(&JsString::from("npm:left-pad@1.2.3"), false)).unwrap();

    let invocation = fs::read_to_string(&log).unwrap();
    assert_eq!(
        invocation.trim(),
        format!(
            "install left-pad@1.2.3 --prefix {}/npm --legacy-peer-deps",
            fixture.agent_dir.to_string_lossy()
        )
    );
    assert!(fixture.agent_path("npm/package.json").ends_with("npm/package.json"));
    assert!(Path::new(&fixture.agent_path("npm/node_modules/left-pad/package.json")).exists());
    let events = events.lock().unwrap();
    assert_eq!(
        events[0].event_type,
        pi_coding_agent::core::package_manager::ProgressType::Start
    );
    assert_eq!(
        events[0].message.as_ref(),
        Some(&JsString::from("Installing npm:left-pad@1.2.3..."))
    );
    assert_eq!(
        events[1].event_type,
        pi_coding_agent::core::package_manager::ProgressType::Complete
    );
}

#[test]
fn bun_and_pnpm_use_their_own_install_arguments() {
    for (name, expected) in [
        ("bun", "install pkg --cwd {root} --omit=peer"),
        (
            "pnpm",
            "install pkg --prefix {root} --config.auto-install-peers=false --config.strict-peer-dependencies=false --config.strict-dep-builds=false",
        ),
    ] {
        let fixture = Fixture::new();
        let log = fixture.temp.path().join(format!("{name}.log"));
        let binary = write_fake_npm(
            &fixture.temp.path().join("bin"),
            name,
            &format!(
                r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
exit 0
"#,
                log.display()
            ),
        );
        let settings =
            FakeSettings::new(Default::default(), Default::default(), false).with_npm_command(vec![binary.into()]);
        let manager = fixture.manager(settings);
        futures_block(manager.install(&JsString::from("npm:pkg"), false)).unwrap();
        let invocation = fs::read_to_string(&log).unwrap();
        let root = fixture.agent_path("npm");
        assert_eq!(invocation.trim(), expected.replace("{root}", &root), "{name} argv");
    }
}

#[test]
fn npm_install_uses_configured_argv_prefixes() {
    let fixture = Fixture::new();
    let log = fixture.temp.path().join("npm.log");
    let npm = write_fake_npm(
        &fixture.temp.path().join("bin"),
        "npm",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
exit 0
"#,
            log.display()
        ),
    );
    let settings = FakeSettings::new(Default::default(), Default::default(), false).with_npm_command(vec![
        "env".to_owned().into(),
        JsString::from("NODE=20"),
        npm.into(),
    ]);
    let manager = fixture.manager(settings);
    futures_block(manager.install(&JsString::from("npm:pkg"), false)).unwrap();
    let invocation = fs::read_to_string(&log).unwrap();
    assert!(invocation.trim().starts_with("install pkg --prefix"));
}

#[test]
fn removes_npm_packages_without_failing_for_missing_roots() {
    let fixture = Fixture::new();
    let log = fixture.temp.path().join("npm.log");
    let npm = write_fake_npm(
        &fixture.temp.path().join("bin"),
        "npm",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
exit 0
"#,
            log.display()
        ),
    );
    let settings = FakeSettings::new(Default::default(), Default::default(), false).with_npm_command(vec![npm.into()]);
    let manager = fixture.manager(settings);

    futures_block(manager.remove(&JsString::from("npm:pkg"), false)).unwrap();
    assert!(!Path::new(&fixture.agent_path("npm")).exists());

    fs::create_dir_all(fixture.agent_dir.join("npm")).unwrap();
    futures_block(manager.remove(&JsString::from("npm:pkg"), false)).unwrap();
    assert_eq!(
        fs::read_to_string(&log).unwrap().trim(),
        format!("uninstall pkg --prefix {}/npm", fixture.agent_dir.to_string_lossy())
    );
}

#[test]
fn installs_local_paths_and_rejects_missing_paths() {
    let fixture = Fixture::new();
    fixture.write("local/ext.ts", "x");
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));

    futures_block(manager.install(&JsString::from("./local"), false)).unwrap();
    let error = futures_block(manager.install(&JsString::from("./missing"), false)).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("Path does not exist: {}", fixture.path("missing"))
    );
}

#[test]
fn untrusted_project_install_is_rejected() {
    let fixture = Fixture::new();
    fixture.write("local/ext.ts", "x");
    let manager = fixture.manager(FakeSettings::new(Default::default(), Default::default(), false));

    let error = futures_block(manager.install(&JsString::from("./local"), true)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Project is not trusted; refusing to access project package storage"
    );
}

#[test]
fn add_and_remove_sources_normalizes_local_paths() {
    let fixture = Fixture::new();
    let settings = Arc::new(FakeSettings::new(Default::default(), Default::default(), false));
    let manager = fixture.manager_with(settings.clone());
    fixture.write("local/ext.ts", "x");

    assert!(
        manager
            .add_source_to_settings(&JsString::from("./local"), false)
            .unwrap()
    );
    assert_eq!(
        settings.global_settings().packages,
        vec![PackageSource::Name("../project/local".to_owned().into())]
    );
    assert!(
        !manager
            .add_source_to_settings(&JsString::from("./local"), false)
            .unwrap()
    );
    assert!(
        manager
            .add_source_to_settings(&JsString::from("npm:left-pad"), false)
            .unwrap()
    );
    assert_eq!(
        settings.global_settings().packages,
        vec![
            PackageSource::Name("../project/local".to_owned().into()),
            PackageSource::Name("npm:left-pad".to_owned().into()),
        ]
    );

    assert!(
        manager
            .remove_source_from_settings(&JsString::from("npm:left-pad@1.2.3"), false)
            .unwrap()
    );
    assert_eq!(
        settings.global_settings().packages,
        vec![PackageSource::Name("../project/local".to_owned().into())]
    );
    assert!(
        !manager
            .remove_source_from_settings(&JsString::from("npm:unknown"), false)
            .unwrap()
    );
}

#[test]
fn replaces_a_filtered_source_while_keeping_its_filters() {
    let fixture = Fixture::new();
    let filter = PackageFilter {
        source: "npm:left-pad@1.0.0".to_owned().into(),
        extensions: Some(vec!["!bad.ts".to_owned().into()]),
        ..PackageFilter::default()
    };
    let settings = Arc::new(FakeSettings::new(
        snapshot(vec![PackageSource::Filter(filter)]),
        Default::default(),
        false,
    ));
    let manager = fixture.manager_with(settings.clone());

    assert!(
        manager
            .add_source_to_settings(&JsString::from("npm:left-pad@2.0.0"), false)
            .unwrap()
    );
    let packages = settings.global_settings().packages;
    let PackageSource::Filter(filter) = &packages[0] else {
        panic!("filtered source expected");
    };
    assert_eq!(filter.source, "npm:left-pad@2.0.0");
    assert_eq!(filter.extensions.as_deref(), Some(&[JsString::from("!bad.ts")][..]));
}

#[test]
fn lists_configured_packages_with_installed_paths() {
    let fixture = Fixture::new();
    fixture.write_agent("npm/node_modules/user-pkg/package.json", "{\"version\":\"1.0.0\"}");
    let settings = FakeSettings::new(
        snapshot(vec![
            PackageSource::Name("npm:user-pkg".to_owned().into()),
            PackageSource::Name("npm:user-missing".to_owned().into()),
        ]),
        snapshot(vec![PackageSource::Filter(PackageFilter {
            source: "npm:project-pkg".to_owned().into(),
            ..PackageFilter::default()
        })]),
        true,
    );
    let manager = fixture.manager(settings);

    let configured = manager.list_configured_packages().unwrap();
    assert_eq!(configured.len(), 3);
    assert_eq!(configured[0].source, "npm:user-pkg");
    assert_eq!(
        configured[0].scope,
        pi_coding_agent::core::package_manager::InstalledScope::User
    );
    assert_eq!(
        configured[0].installed_path.as_ref(),
        Some(&JsString::from(
            fixture.agent_path("npm/node_modules/user-pkg").as_str()
        ))
    );
    assert_eq!(configured[1].installed_path, None);
    assert!(configured[2].filtered);
}

#[test]
fn updates_unpinned_npm_packages_and_skips_pinned_ones() {
    let fixture = Fixture::new();
    fixture.write_agent(
        "npm/node_modules/old-pkg/package.json",
        "{\"name\":\"old-pkg\",\"version\":\"1.0.0\"}",
    );
    fixture.write_agent(
        "npm/node_modules/pinned/package.json",
        "{\"name\":\"pinned\",\"version\":\"1.0.0\"}",
    );
    let log = fixture.temp.path().join("npm.log");
    let npm = write_fake_npm(
        &fixture.temp.path().join("bin"),
        "npm",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "{}"
if [ "$1" = "view" ]; then printf '"2.0.0"'; fi
exit 0
"#,
            log.display()
        ),
    );
    let settings = FakeSettings::new(
        snapshot(vec![
            PackageSource::Name("npm:old-pkg".to_owned().into()),
            PackageSource::Name("npm:pinned@1.0.0".to_owned().into()),
        ]),
        Default::default(),
        false,
    )
    .with_npm_command(vec![npm.into()]);
    let manager = fixture.manager(settings);

    futures_block(manager.update(None)).unwrap();
    let invocations = fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = invocations.lines().collect();
    assert!(lines.contains(&"view old-pkg version --json"), "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("install old-pkg@latest --prefix")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("pinned")),
        "pinned packages must not be updated: {lines:?}"
    );
}

#[test]
fn update_reports_missing_sources_with_suggestions() {
    let fixture = Fixture::new();
    let settings = FakeSettings::new(
        snapshot(vec![PackageSource::Name("npm:left-pad".to_owned().into())]),
        Default::default(),
        false,
    );
    let manager = fixture.manager(settings);

    let error = futures_block(manager.update(Some(&JsString::from("left-pad")))).unwrap_err();
    assert_eq!(
        error.to_string(),
        "No matching package found for left-pad. Did you mean npm:left-pad?"
    );
    let error = futures_block(manager.update(Some(&JsString::from("npm:unknown")))).unwrap_err();
    assert_eq!(error.to_string(), "No matching package found for npm:unknown");
}

#[test]
fn check_for_available_updates_reports_unpinned_installed_packages() {
    let fixture = Fixture::new();
    fixture.write_agent(
        "npm/node_modules/old-pkg/package.json",
        "{\"name\":\"old-pkg\",\"version\":\"1.0.0\"}",
    );
    let npm = write_fake_npm(
        &fixture.temp.path().join("bin"),
        "npm",
        r#"#!/bin/sh
if [ "$1" = "view" ]; then printf '"2.0.0"'; fi
exit 0
"#,
    );
    let settings = FakeSettings::new(
        snapshot(vec![
            PackageSource::Name("npm:old-pkg".to_owned().into()),
            PackageSource::Name("npm:current@2.0.0".to_owned().into()),
            PackageSource::Name("npm:absent".to_owned().into()),
            PackageSource::Name("npm:pinned@1.0.0".to_owned().into()),
        ]),
        Default::default(),
        false,
    )
    .with_npm_command(vec![npm.into()]);
    let manager = fixture.manager(settings);

    let updates = futures_block(manager.check_for_available_updates()).unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].source, "npm:old-pkg");
    assert_eq!(updates[0].display_name, "old-pkg");
}

#[test]
fn resolve_extension_sources_uses_temporary_and_project_scopes() {
    let fixture = Fixture::new();
    fixture.write(".pi/extensions/ext.ts", "x");
    let settings = FakeSettings::new(Default::default(), Default::default(), true);
    let manager = fixture.manager(settings);

    let resolved =
        futures_block(manager.resolve_extension_sources(&["extensions/ext.ts".to_owned().into()], true, false))
            .unwrap();
    assert!(
        resolved
            .extensions
            .iter()
            .any(|resource| ends_with(&resource.path, "ext.ts"))
    );
    assert_eq!(resolved.extensions[0].metadata.scope, SourceScope::Project);
}

fn futures_block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

#[test]
fn settings_manager_store_adapts_the_ported_settings_manager() {
    use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};

    let mut settings = serde_json::Map::new();
    settings.insert("packages".to_owned(), json!(["npm:my-pkg"]));
    settings.insert("extensions".to_owned(), json!(["extensions"]));
    settings.insert("npmCommand".to_owned(), json!(["mise", "exec", "node@20", "--", "npm"]));
    let settings_manager = SettingsManager::in_memory(
        pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(settings))
            .as_object()
            .unwrap()
            .clone(),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    );
    let store = Arc::new(SettingsManagerStore::new(settings_manager));
    let fixture = Fixture::new();
    let manager = fixture.manager_with(store.clone());

    assert!(
        manager
            .add_source_to_settings(&JsString::from("npm:left-pad"), false)
            .unwrap()
    );
    assert_eq!(
        store.with(|settings_manager| settings_manager.get_packages().unwrap()),
        vec![
            PackageSource::Name("npm:my-pkg".to_owned().into()),
            PackageSource::Name("npm:left-pad".to_owned().into()),
        ]
    );
    assert_eq!(
        store.with(|manager| manager.get_npm_command()),
        Some(vec![
            JsString::from("mise"),
            JsString::from("exec"),
            JsString::from("node@20"),
            JsString::from("--"),
            JsString::from("npm")
        ])
    );
    assert!(!manager.list_configured_packages().unwrap().is_empty());
}

#[test]
fn missing_npm_binary_reports_the_node_spawn_error_text() {
    let fixture = Fixture::new();
    let missing = fixture.temp.path().join("missing-npm");
    let settings = FakeSettings::new(Default::default(), Default::default(), false)
        .with_npm_command(vec![missing.to_string_lossy().into_owned().into()]);
    let manager = fixture.manager(settings);

    let error = futures_block(manager.install(&JsString::from("npm:left-pad"), false)).expect_err("spawn must fail");

    assert_eq!(error.to_string(), format!("spawn {} ENOENT", missing.display()));
}

#[test]
fn non_executable_npm_binary_reports_the_node_spawn_error_text() {
    let fixture = Fixture::new();
    let binary = fixture.temp.path().join("npm");
    fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let settings = FakeSettings::new(Default::default(), Default::default(), false)
        .with_npm_command(vec![binary.to_string_lossy().into_owned().into()]);
    let manager = fixture.manager(settings);

    let error = futures_block(manager.install(&JsString::from("npm:left-pad"), false)).expect_err("spawn must fail");

    assert_eq!(error.to_string(), format!("spawn {} EACCES", binary.display()));
}
