//! Differential proof for `config.ts`: install detection, self-update commands, and the
//! layout-independent path getters, compared byte-for-byte with the pinned TypeScript module.

#[path = "support/config_probe.rs"]
mod config_probe;
use crate::shared_executable_script as executable_script;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use pi_coding_agent::config::{
    InstallContext, SelfUpdatePackageTarget, detect_install_method, get_self_update_command,
    get_self_update_unavailable_instruction, get_update_instruction,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const PACKAGE: &str = "@earendil-works/pi-coding-agent";

#[test]
fn advertised_version_matches_frozen_package() {
    let source_package: Value =
        serde_json::from_slice(&fs::read(source_root().join("packages/coding-agent/package.json")).unwrap()).unwrap();
    assert_eq!(
        pi_coding_agent::config::version(),
        source_package["version"].as_str().unwrap()
    );
}

#[test]
#[ignore = "child probe entry point"]
fn config_probe_child() {
    config_probe::run_child_probe();
}

struct OracleProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl OracleProcess {
    fn spawn(package_dir: &Path) -> Self {
        let root = source_root();
        let mut child = Command::new(root.join("node_modules/.bin/tsx"))
            .arg("--tsconfig")
            .arg(root.join("tsconfig.json"))
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/config-oracle.ts"))
            .arg(root.join("packages/coding-agent/src/config.ts"))
            .env_clear()
            .env("PI_PACKAGE_DIR", package_dir)
            .env("HOME", "/home/tester")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("tsx must start the config oracle");
        let stdin = child.stdin.take().expect("oracle stdin");
        let stdout = BufReader::new(child.stdout.take().expect("oracle stdout"));
        Self { child, stdin, stdout }
    }

    fn request(&mut self, request: &Value) -> Value {
        serde_json::to_writer(&mut self.stdin, request).expect("request serializes");
        self.stdin.write_all(b"\n").unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("oracle response");
        serde_json::from_str(&line).expect("oracle response must be JSON")
    }
}

impl Drop for OracleProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn source_root() -> PathBuf {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .unwrap();
    PathBuf::from(lock["worktree"].as_str().unwrap())
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-config-diff-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_package(dir: &Path, content: &str) {
    fs::write(dir.join("package.json"), content).unwrap();
}

fn write_script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    executable_script::write_executable_script(&dir.join(name), body);
}

fn env_pairs(env: &BTreeMap<String, String>) -> Vec<(&str, OsString)> {
    env.iter()
        .map(|(name, value)| (name.as_str(), OsString::from(value)))
        .collect()
}

fn strip_layout_specific(mut payload: Value) -> Value {
    // The compiled-binary runtime ships theme/, export-html/, and assets/ next to the executable;
    // the Node runtime resolves them under src/ or dist/. Focused tests cover the Rust layout.
    if let Some(object) = payload.as_object_mut() {
        for key in ["themesDir", "exportTemplateDir", "interactiveAssetsDir", "bundledAsset"] {
            object.remove(key);
        }
    }
    payload
}

#[test]
fn paths_match_the_pinned_typescript_module() {
    let package = temp_dir("paths-package");
    write_package(
        &package,
        "{\"name\":\"@acme/agent\",\"version\":\"9.9.9\",\"piConfig\":{\"name\":\"tau\",\"configDir\":\".tau\"}}",
    );
    let scenarios: Vec<BTreeMap<String, String>> = vec![
        BTreeMap::from([("HOME".to_owned(), "/home/tester".to_owned())]),
        BTreeMap::from([
            ("HOME".to_owned(), "/home/tester".to_owned()),
            ("XDG_CONFIG_HOME".to_owned(), "/tmp/xdg-config".to_owned()),
            ("XDG_STATE_HOME".to_owned(), "/tmp/xdg-state".to_owned()),
        ]),
        BTreeMap::from([
            ("HOME".to_owned(), "/home/tester".to_owned()),
            ("TAU_CODING_AGENT_DIR".to_owned(), "~/custom-agent".to_owned()),
            ("TAU_CODING_AGENT_STATE_DIR".to_owned(), "~/custom-state".to_owned()),
            ("XDG_CONFIG_HOME".to_owned(), "/tmp/ignored".to_owned()),
        ]),
        BTreeMap::from([
            ("HOME".to_owned(), "/home/tester".to_owned()),
            ("PI_SHARE_VIEWER_URL".to_owned(), "https://example.test/s/".to_owned()),
        ]),
    ];

    let mut oracle = OracleProcess::spawn(&package);
    for (index, env) in scenarios.iter().enumerate() {
        let mut request_env = env.clone();
        request_env.insert("PI_PACKAGE_DIR".to_owned(), package.display().to_string());
        let expected = oracle.request(&json!({"id": index, "op": "paths", "env": request_env}));
        assert_eq!(expected["ok"], true, "oracle failed: {expected}");
        let mut probe_env = env_pairs(env);
        probe_env.push(("PI_PACKAGE_DIR", package.display().to_string().into()));
        let actual = config_probe::spawn_child_probe("paths", &probe_env);
        assert_eq!(
            strip_layout_specific(actual),
            strip_layout_specific(expected["output"].clone()),
            "scenario {index}: {env:?}"
        );
    }
}

struct InstallScenario<'a> {
    name: &'a str,
    context: InstallContext,
    package_name: &'a str,
    npm_command: Option<&'a [String]>,
    target: Option<&'a SelfUpdatePackageTarget>,
}

fn context(
    package_dir: &Path,
    exec_path: &str,
    platform: &str,
    path_env: &Path,
    env: &BTreeMap<String, String>,
) -> InstallContext {
    InstallContext {
        package_dir: package_dir.to_owned(),
        exec_path: exec_path.to_owned(),
        entrypoint: None,
        is_bun_binary: false,
        is_bun_runtime: false,
        platform: platform.to_owned(),
        home: PathBuf::from("/home/tester"),
        path_env: Some(path_env.display().to_string()),
        env: env.clone(),
    }
}

/// Fixture text is well-formed, so the oracle's JSON strings compare against UTF-8 text.
fn text(value: &JsString) -> String {
    value.to_utf8().expect("well-formed fixture text")
}

fn texts(values: &[JsString]) -> Vec<String> {
    values.iter().map(text).collect()
}

fn install_payload(
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Value {
    let package_name = JsString::from(package_name);
    let npm_command: Option<Vec<JsString>> = npm_command.map(|args| args.iter().map(JsString::from).collect());
    let npm_command = npm_command.as_deref();
    let command = get_self_update_command(context, &package_name, npm_command, target).unwrap();
    json!({
        "method": detect_install_method(context).as_str(),
        "command": command.map(|command| json!({
            "command": text(&command.command),
            "args": texts(&command.args),
            "display": text(&command.display),
            "steps": command.steps.map(|steps| steps.iter().map(|step| json!({
                "command": text(&step.command),
                "args": texts(&step.args),
                "display": text(&step.display),
            })).collect::<Vec<_>>()),
        })),
        "updateInstruction": text(&get_update_instruction(context, &package_name).unwrap()),
        "unavailableInstruction": text(&get_self_update_unavailable_instruction(context, &package_name, npm_command, target).unwrap()),
    })
}

fn install_request(
    id: usize,
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Value {
    let mut env = context.env.clone();
    env.insert("PI_PACKAGE_DIR".to_owned(), context.package_dir.display().to_string());
    if let Some(path) = &context.path_env {
        env.insert("PATH".to_owned(), path.clone());
    }
    json!({
        "id": id,
        "op": "install",
        "env": env,
        "execPath": context.exec_path,
        "argv1": context.entrypoint,
        "platform": context.platform,
        "packageName": package_name,
        "npmCommand": npm_command,
        "target": target.map(|target| json!({
            "packageName": text(&target.package_name),
            "installSpec": target.install_spec.as_ref().map(text),
        })),
    })
}

#[test]
fn install_commands_match_the_pinned_typescript_module() {
    let package = temp_dir("install-package");
    write_package(
        &package,
        "{\"name\":\"@earendil-works/pi-coding-agent\",\"version\":\"0.0.0\"}",
    );
    let empty_path = temp_dir("empty-path");

    let npm_prefix = temp_dir("npm-prefix");
    let npm_package_dir = npm_prefix.join("lib/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&npm_package_dir).unwrap();
    let npm_exec = npm_package_dir.join("dist/cli.js").display().to_string();
    let spaced_prefix = temp_dir("npm prefix ");
    let spaced_package_dir = spaced_prefix.join("lib/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&spaced_package_dir).unwrap();
    let spaced_exec = spaced_package_dir.join("dist/cli.js").display().to_string();

    let npm_bin = temp_dir("npm-bin");
    let npm_script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--prefix\" ] && [ \"$3\" = \"root\" ] && [ \"$4\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}/lib/node_modules'\n\texit 0\nfi\nexit 1\n",
        npm_prefix.display()
    );
    write_script(&npm_bin, "npm", &npm_script);

    let bun_home = temp_dir("bun-home");
    let bun_package_dir = bun_home.join(".bun/install/global/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&bun_package_dir).unwrap();
    let bun_exec = bun_package_dir.join("dist/cli.js").display().to_string();
    let bun_bin = temp_dir("bun-bin");
    let bun_script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"pm\" ] && [ \"$2\" = \"bin\" ] && [ \"$3\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}/bin'\n\texit 0\nfi\nexit 1\n",
        bun_home.display()
    );
    write_script(&bun_bin, "bun", &bun_script);

    let pnpm_prefix = temp_dir("pnpm-prefix");
    let pnpm_root = pnpm_prefix.join("pnpm/global/5/node_modules");
    let pnpm_package_dir = pnpm_root.join("@mariozechner/pi-coding-agent");
    fs::create_dir_all(&pnpm_package_dir).unwrap();
    let pnpm_exec = pnpm_root
        .join(".pnpm/@mariozechner+pi-coding-agent@0.0.0/node_modules/@mariozechner/pi-coding-agent/dist/cli.js")
        .display()
        .to_string();
    let pnpm_bin = temp_dir("pnpm-bin");
    let pnpm_script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"root\" ] && [ \"$2\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        pnpm_root.display()
    );
    write_script(&pnpm_bin, "pnpm", &pnpm_script);

    let yarn_global = temp_dir("yarn-global");
    let yarn_package_dir = yarn_global.join("node_modules/@mariozechner/pi-coding-agent");
    fs::create_dir_all(&yarn_package_dir).unwrap();
    let yarn_exec = yarn_global
        .join(".yarn/@mariozechner/pi-coding-agent/dist/cli.js")
        .display()
        .to_string();
    let yarn_bin = temp_dir("yarn-bin");
    let yarn_script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"global\" ] && [ \"$2\" = \"dir\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        yarn_global.display()
    );
    write_script(&yarn_bin, "yarn", &yarn_script);

    let pnpm_home_prefix = temp_dir("pnpm-home-prefix");
    let pnpm_home_package = pnpm_home_prefix
        .join("pnpm/global/5/.pnpm/@earendil-works+pi-coding-agent@1.0.0/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&pnpm_home_package).unwrap();
    let pnpm_home_exec = pnpm_home_package.join("dist/cli.js").display().to_string();
    let mut pnpm_home_env = BTreeMap::new();
    pnpm_home_env.insert("PNPM_HOME".to_owned(), "/custom/pnpm-home".to_owned());

    let npm_command = [
        "npm".to_owned(),
        "--prefix".to_owned(),
        npm_prefix.display().to_string(),
    ];
    let renamed_npm = SelfUpdatePackageTarget::from("@new-scope/pi");
    let exact_npm = SelfUpdatePackageTarget::with_install_spec(PACKAGE, format!("{PACKAGE}@1.2.3"));
    let empty_env = BTreeMap::new();

    let scenarios = vec![
        InstallScenario {
            name: "npm prefix",
            context: context(&npm_package_dir, &npm_exec, "linux", &empty_path, &empty_env),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "npm exact",
            context: context(&npm_package_dir, &npm_exec, "linux", &empty_path, &empty_env),
            package_name: PACKAGE,
            npm_command: None,
            target: Some(&exact_npm),
        },
        InstallScenario {
            name: "npm renamed",
            context: context(&npm_package_dir, &npm_exec, "linux", &empty_path, &empty_env),
            package_name: "@mariozechner/pi-coding-agent",
            npm_command: None,
            target: Some(&renamed_npm),
        },
        InstallScenario {
            name: "npm configured",
            context: context(&npm_package_dir, &npm_exec, "linux", &npm_bin, &empty_env),
            package_name: PACKAGE,
            npm_command: Some(&npm_command),
            target: None,
        },
        InstallScenario {
            name: "npm prefix with spaces",
            context: context(&spaced_package_dir, &spaced_exec, "linux", &empty_path, &empty_env),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "windows npm custom prefix",
            context: context(
                Path::new("C:\\Users\\Admin\\npm prefix\\node_modules\\@earendil-works\\pi-coding-agent"),
                "C:\\Users\\Admin\\npm prefix\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js",
                "win32",
                &empty_path,
                &empty_env,
            ),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "bun global",
            context: context(&bun_package_dir, &bun_exec, "linux", &bun_bin, &empty_env),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "bun renamed",
            context: context(&bun_package_dir, &bun_exec, "linux", &bun_bin, &empty_env),
            package_name: "@mariozechner/pi-coding-agent",
            npm_command: None,
            target: Some(&renamed_npm),
        },
        InstallScenario {
            name: "pnpm renamed",
            context: context(&pnpm_package_dir, &pnpm_exec, "linux", &pnpm_bin, &empty_env),
            package_name: "@mariozechner/pi-coding-agent",
            npm_command: None,
            target: Some(&renamed_npm),
        },
        InstallScenario {
            name: "yarn renamed",
            context: context(&yarn_package_dir, &yarn_exec, "linux", &yarn_bin, &empty_env),
            package_name: "@mariozechner/pi-coding-agent",
            npm_command: None,
            target: Some(&renamed_npm),
        },
        InstallScenario {
            name: "pnpm home",
            context: context(
                &pnpm_home_package,
                &pnpm_home_exec,
                "linux",
                &empty_path,
                &pnpm_home_env,
            ),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "pnpm windows detection",
            context: context(
                &package,
                "C:\\Users\\Admin\\Documents\\pnpm-repository\\global\\5\\.pnpm\\@earendil-works+pi-coding-agent@0.67.68\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js",
                "win32",
                &empty_path,
                &empty_env,
            ),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
        InstallScenario {
            name: "unknown",
            context: context(&package, "/usr/local/bin/node", "linux", &empty_path, &empty_env),
            package_name: PACKAGE,
            npm_command: None,
            target: None,
        },
    ];

    let mut oracle = OracleProcess::spawn(&package);
    for (index, scenario) in scenarios.iter().enumerate() {
        let request = install_request(
            index,
            &scenario.context,
            scenario.package_name,
            scenario.npm_command,
            scenario.target,
        );
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "{}: oracle failed: {response}", scenario.name);
        let actual = install_payload(
            &scenario.context,
            scenario.package_name,
            scenario.npm_command,
            scenario.target,
        );
        assert_eq!(actual, response["output"], "scenario {}", scenario.name);
    }
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
