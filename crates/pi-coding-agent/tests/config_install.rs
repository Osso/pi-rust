//! `config.ts` install-method detection and self-update command construction. Ports every case of
//! `packages/coding-agent/test/config.test.ts` using an injected install context (package dir,
//! exec path, argv entrypoint, platform, PATH) instead of mutating process globals.

use crate::shared_executable_script as executable_script;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::config::{self, InstallContext, InstallMethod, SelfUpdatePackageTarget, detect_install_method};
use pi_js_compat::js_json::JsString;

const PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// Text view of a native self-update command step; every fixture here is well-formed text.
#[derive(Debug, PartialEq)]
struct TextStep {
    command: String,
    args: Vec<String>,
    display: String,
}

#[derive(Debug, PartialEq)]
struct TextCommand {
    command: String,
    args: Vec<String>,
    display: String,
    steps: Option<Vec<TextStep>>,
}

fn text(value: &JsString) -> String {
    value.to_utf8().expect("well-formed fixture text")
}

fn texts(values: &[JsString]) -> Vec<String> {
    values.iter().map(text).collect()
}

fn native_npm_command(npm_command: Option<&[String]>) -> Option<Vec<JsString>> {
    npm_command.map(|args| args.iter().map(JsString::from).collect())
}

fn get_self_update_command(
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> Option<TextCommand> {
    let npm_command = native_npm_command(npm_command);
    let command =
        config::get_self_update_command(context, &package_name.into(), npm_command.as_deref(), target).unwrap()?;
    Some(TextCommand {
        command: text(&command.command),
        args: texts(&command.args),
        display: text(&command.display),
        steps: command.steps.map(|steps| {
            steps
                .iter()
                .map(|step| TextStep {
                    command: text(&step.command),
                    args: texts(&step.args),
                    display: text(&step.display),
                })
                .collect()
        }),
    })
}

fn get_update_instruction(context: &InstallContext, package_name: &str) -> String {
    text(&config::get_update_instruction(context, &package_name.into()).unwrap())
}

fn get_self_update_unavailable_instruction(
    context: &InstallContext,
    package_name: &str,
    npm_command: Option<&[String]>,
    target: Option<&SelfUpdatePackageTarget>,
) -> String {
    let npm_command = native_npm_command(npm_command);
    text(
        &config::get_self_update_unavailable_instruction(context, &package_name.into(), npm_command.as_deref(), target)
            .unwrap(),
    )
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-install-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn context(package_dir: &Path, exec_path: &str) -> InstallContext {
    InstallContext {
        package_dir: package_dir.to_owned(),
        exec_path: exec_path.to_owned(),
        entrypoint: None,
        is_bun_binary: false,
        is_bun_runtime: false,
        platform: "linux".to_owned(),
        home: PathBuf::from("/home/tester"),
        path_env: None,
        env: BTreeMap::new(),
    }
}

/// An empty PATH so `npm root -g`, `pnpm root -g`, `yarn global dir`, and `bun pm bin -g` fail like
/// they do on a machine without the package manager installed.
fn with_empty_path(mut context: InstallContext) -> InstallContext {
    let dir = temp_dir("empty-path");
    context.path_env = Some(dir.display().to_string());
    context
}

fn write_script(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    executable_script::write_executable_script(&dir.join(name), body);
}

fn create_npm_prefix_install(template: &str) -> (PathBuf, PathBuf) {
    let prefix = temp_dir(template);
    let package_dir = prefix.join("lib/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    (prefix, package_dir)
}

#[test]
fn detects_pnpm_from_windows_pnpm_install_paths() {
    let package_dir = temp_dir("pnpm-win-package");
    let mut context = with_empty_path(context(
        &package_dir,
        "C:\\Users\\Admin\\Documents\\pnpm-repository\\global\\5\\.pnpm\\@earendil-works+pi-coding-agent@0.67.68\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js",
    ));
    context.platform = "win32".to_owned();

    assert_eq!(detect_install_method(&context), InstallMethod::Pnpm);
    assert_eq!(
        get_update_instruction(&context, PACKAGE),
        "Run: pnpm install -g --ignore-scripts --config.minimumReleaseAge=0 @earendil-works/pi-coding-agent"
    );
}

#[test]
fn does_not_self_update_unknown_wrapper_installs() {
    let package_dir = temp_dir("unknown-package");
    let context = with_empty_path(context(&package_dir, "/usr/local/bin/node"));

    assert_eq!(detect_install_method(&context), InstallMethod::Unknown);
    assert_eq!(get_self_update_command(&context, PACKAGE, None, None), None);
    assert_eq!(
        get_update_instruction(&context, PACKAGE),
        "Update @earendil-works/pi-coding-agent using the package manager, wrapper, or source checkout that provides this installation."
    );
}

#[test]
fn self_updates_npm_installs_from_custom_prefixes() {
    let (prefix, package_dir) = create_npm_prefix_install("npm-prefix");
    let context = with_empty_path(context(
        &package_dir,
        &package_dir.join("dist/cli.js").display().to_string(),
    ));

    let command = get_self_update_command(&context, PACKAGE, None, None).expect("npm install must self-update");
    assert_eq!(detect_install_method(&context), InstallMethod::Npm);
    let prefix_arg = prefix.display().to_string();
    assert_eq!(command.command, "npm");
    assert_eq!(
        command.args,
        [
            "--prefix",
            &prefix_arg,
            "install",
            "-g",
            "--ignore-scripts",
            "--min-release-age=0",
            "@earendil-works/pi-coding-agent",
        ]
    );
    assert_eq!(
        command.display,
        format!(
            "npm --prefix {prefix_arg} install -g --ignore-scripts --min-release-age=0 @earendil-works/pi-coding-agent"
        )
    );
    assert_eq!(command.steps, None);
}

#[test]
fn self_updates_exact_npm_versions_without_uninstalling() {
    let (prefix, package_dir) = create_npm_prefix_install("npm-exact");
    let context = with_empty_path(context(
        &package_dir,
        &package_dir.join("dist/cli.js").display().to_string(),
    ));
    let target = SelfUpdatePackageTarget::with_install_spec(PACKAGE, format!("{PACKAGE}@1.2.3"));

    let command = get_self_update_command(&context, PACKAGE, None, Some(&target)).expect("exact version update");
    assert_eq!(
        command.args,
        [
            "--prefix",
            &prefix.display().to_string(),
            "install",
            "-g",
            "--ignore-scripts",
            "--min-release-age=0",
            "@earendil-works/pi-coding-agent@1.2.3",
        ]
    );
    assert_eq!(command.steps, None);
    assert_eq!(
        command.display,
        format!(
            "npm --prefix {} install -g --ignore-scripts --min-release-age=0 {PACKAGE}@1.2.3",
            prefix.display()
        )
    );
}

#[test]
fn self_updates_renamed_npm_packages_from_the_current_prefix() {
    let (prefix, package_dir) = create_npm_prefix_install("npm-renamed");
    let context = with_empty_path(context(
        &package_dir,
        &package_dir.join("dist/cli.js").display().to_string(),
    ));

    let command = get_self_update_command(
        &context,
        "@mariozechner/pi-coding-agent",
        None,
        Some(&SelfUpdatePackageTarget::from("@new-scope/pi")),
    )
    .expect("renamed package update");
    let prefix_arg = prefix.display().to_string();
    assert_eq!(
        command.display,
        format!(
            "npm --prefix {prefix_arg} uninstall -g @mariozechner/pi-coding-agent && npm --prefix {prefix_arg} install -g --ignore-scripts --min-release-age=0 @new-scope/pi"
        )
    );
    let steps = command.steps.expect("renamed packages uninstall first");
    assert_eq!(steps.len(), 2);
    assert_eq!(
        steps[0].args,
        [
            "--prefix",
            &prefix_arg,
            "uninstall",
            "-g",
            "@mariozechner/pi-coding-agent"
        ]
    );
    assert_eq!(
        steps[0].display,
        format!("npm --prefix {prefix_arg} uninstall -g @mariozechner/pi-coding-agent")
    );
    assert_eq!(
        steps[1].args,
        [
            "--prefix",
            &prefix_arg,
            "install",
            "-g",
            "--ignore-scripts",
            "--min-release-age=0",
            "@new-scope/pi"
        ]
    );
}

#[test]
fn self_update_respects_the_configured_npm_command_and_treats_empty_as_unset() {
    let (prefix, package_dir) = create_npm_prefix_install("npm-command");
    let bin_dir = temp_dir("npm-command-bin");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"--prefix\" ] && [ \"$3\" = \"root\" ] && [ \"$4\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}/lib/node_modules'\n\texit 0\nfi\nexit 1\n",
        prefix.display()
    );
    write_script(&bin_dir, "npm", &script);
    let mut context = context(&package_dir, &package_dir.join("dist/cli.js").display().to_string());
    context.path_env = Some(bin_dir.display().to_string());
    let prefix_arg = prefix.display().to_string();
    let configured = ["npm".to_owned(), "--prefix".to_owned(), prefix_arg.clone()];

    let command = get_self_update_command(&context, PACKAGE, Some(&configured), None).expect("configured npm");
    assert_eq!(
        command.args,
        [
            "--prefix",
            &prefix_arg,
            "install",
            "-g",
            "--ignore-scripts",
            "--min-release-age=0",
            "@earendil-works/pi-coding-agent",
        ]
    );

    let command = get_self_update_command(&context, PACKAGE, Some(&[]), None).expect("empty npmCommand");
    assert_eq!(command.args[0], "--prefix");
}

#[test]
fn quotes_npm_self_update_display_paths_with_spaces() {
    let (prefix, package_dir) = create_npm_prefix_install("npm prefix ");
    let context = with_empty_path(context(
        &package_dir,
        &package_dir.join("dist/cli.js").display().to_string(),
    ));

    let command = get_self_update_command(&context, PACKAGE, None, None).expect("npm install");
    assert_eq!(
        command.display,
        format!(
            "npm --prefix \"{}\" install -g --ignore-scripts --min-release-age=0 @earendil-works/pi-coding-agent",
            prefix.display()
        )
    );
}

#[test]
fn does_not_infer_windows_npm_custom_prefixes() {
    let package_dir = PathBuf::from("C:\\Users\\Admin\\npm prefix\\node_modules\\@earendil-works\\pi-coding-agent");
    let mut context = with_empty_path(context(
        &package_dir,
        "C:\\Users\\Admin\\npm prefix\\node_modules\\@earendil-works\\pi-coding-agent\\dist\\cli.js",
    ));
    context.platform = "win32".to_owned();

    assert_eq!(detect_install_method(&context), InstallMethod::Npm);
    assert_eq!(
        get_update_instruction(&context, PACKAGE),
        "Run: npm install -g --ignore-scripts --min-release-age=0 @earendil-works/pi-coding-agent"
    );
}

#[test]
fn self_updates_bun_global_installs_from_bun_pm_bin() {
    let temp = temp_dir("bun-global");
    let package_dir = temp.join(".bun/install/global/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    let bun_bin = temp.join(".bun/bin");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"pm\" ] && [ \"$2\" = \"bin\" ] && [ \"$3\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        bun_bin.display()
    );
    write_script(&bun_bin, "bun", &script);

    let mut context = context(&package_dir, &package_dir.join("dist/cli.js").display().to_string());
    context.path_env = Some(bun_bin.display().to_string());

    let command = get_self_update_command(&context, PACKAGE, None, None).expect("bun install");
    assert_eq!(detect_install_method(&context), InstallMethod::Bun);
    assert_eq!(
        command.display,
        "bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent"
    );
    assert_eq!(command.steps, None);

    let renamed = get_self_update_command(
        &context,
        "@mariozechner/pi-coding-agent",
        None,
        Some(&SelfUpdatePackageTarget::from("@new-scope/pi")),
    )
    .expect("renamed bun install");
    assert_eq!(
        renamed.display,
        "bun uninstall -g @mariozechner/pi-coding-agent && bun install -g --ignore-scripts --minimum-release-age=0 @new-scope/pi"
    );
}

#[test]
fn self_updates_renamed_pnpm_global_installs_by_removing_the_old_package() {
    let temp = temp_dir("pnpm-global");
    let root = temp.join("pnpm/global/5/node_modules");
    let package_dir = root.join("@mariozechner/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    let bin_dir = temp.join("bin");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"root\" ] && [ \"$2\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        root.display()
    );
    write_script(&bin_dir, "pnpm", &script);

    let mut context = context(
        &package_dir,
        &root
            .join(".pnpm/@mariozechner+pi-coding-agent@0.0.0/node_modules/@mariozechner/pi-coding-agent/dist/cli.js")
            .display()
            .to_string(),
    );
    context.path_env = Some(bin_dir.display().to_string());

    let command = get_self_update_command(
        &context,
        "@mariozechner/pi-coding-agent",
        None,
        Some(&SelfUpdatePackageTarget::from("@new-scope/pi")),
    )
    .expect("renamed pnpm install");
    assert_eq!(detect_install_method(&context), InstallMethod::Pnpm);
    assert_eq!(
        command.display,
        "pnpm remove -g @mariozechner/pi-coding-agent && pnpm install -g --ignore-scripts --config.minimumReleaseAge=0 @new-scope/pi"
    );
    assert_eq!(
        command.steps.expect("two steps")[0].args,
        ["remove", "-g", "@mariozechner/pi-coding-agent"]
    );
}

#[test]
fn self_updates_pnpm_v11_global_installs_resolved_through_the_store() {
    let temp = temp_dir("pnpm11");
    let root = temp.join("Library/pnpm/global/v11");
    let global_package_dir = root.join("11e9a/node_modules/@earendil-works/pi-coding-agent");
    let store_package_dir = temp.join(
        "Library/pnpm/store/v11/links/@earendil-works/pi-coding-agent/0.75.0/hash/node_modules/@earendil-works/pi-coding-agent",
    );
    fs::create_dir_all(&global_package_dir).unwrap();
    fs::create_dir_all(&store_package_dir).unwrap();
    fs::write(global_package_dir.join("package.json"), "{}").unwrap();
    let bin_dir = temp.join("bin");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"root\" ] && [ \"$2\" = \"-g\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        root.display()
    );
    write_script(&bin_dir, "pnpm", &script);

    let mut context = context(
        &store_package_dir,
        &store_package_dir.join("dist/cli.js").display().to_string(),
    );
    context.entrypoint = Some(global_package_dir.join("dist/cli.js").display().to_string());
    context.path_env = Some(bin_dir.display().to_string());

    let command = get_self_update_command(&context, PACKAGE, None, None).expect("pnpm v11 install");
    assert_eq!(detect_install_method(&context), InstallMethod::Pnpm);
    assert_eq!(
        command.display,
        "pnpm install -g --ignore-scripts --config.minimumReleaseAge=0 @earendil-works/pi-coding-agent"
    );
}

#[test]
fn self_updates_renamed_yarn_global_installs_by_removing_the_old_package() {
    let temp = temp_dir("yarn-global");
    let global_dir = temp.join("yarn/global");
    let package_dir = global_dir.join("node_modules/@mariozechner/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    let bin_dir = temp.join("bin");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = \"global\" ] && [ \"$2\" = \"dir\" ]; then\n\tprintf '%s\\n' '{}'\n\texit 0\nfi\nexit 1\n",
        global_dir.display()
    );
    write_script(&bin_dir, "yarn", &script);

    let mut context = context(
        &package_dir,
        &global_dir
            .join(".yarn/@mariozechner/pi-coding-agent/dist/cli.js")
            .display()
            .to_string(),
    );
    context.path_env = Some(bin_dir.display().to_string());

    let command = get_self_update_command(
        &context,
        "@mariozechner/pi-coding-agent",
        None,
        Some(&SelfUpdatePackageTarget::from("@new-scope/pi")),
    )
    .expect("renamed yarn install");
    assert_eq!(detect_install_method(&context), InstallMethod::Yarn);
    assert_eq!(
        command.display,
        "yarn global remove @mariozechner/pi-coding-agent && yarn global add --ignore-scripts @new-scope/pi"
    );
    assert_eq!(
        command.steps.expect("two steps")[0].args,
        ["global", "remove", "@mariozechner/pi-coding-agent"]
    );
}

#[test]
fn pnpm_uses_the_configured_pnpm_home_for_the_global_bin_dir() {
    let temp = temp_dir("pnpm-home");
    let root = temp.join("pnpm/global/5/node_modules");
    let package_dir = root.join("@earendil-works/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    let bin_dir = temp.join("bin");
    write_script(&bin_dir, "pnpm", "#!/bin/sh\nexit 1\n");

    let pnpm_package_dir = temp
        .join("pnpm/global/5/.pnpm/@earendil-works+pi-coding-agent@1.0.0/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&pnpm_package_dir).unwrap();
    let mut context = context(
        &pnpm_package_dir,
        &pnpm_package_dir.join("dist/cli.js").display().to_string(),
    );
    context.path_env = Some(bin_dir.display().to_string());
    context
        .env
        .insert("PNPM_HOME".to_owned(), "/custom/pnpm-home".to_owned());

    let command = get_self_update_command(&context, PACKAGE, None, None).expect("pnpm install");
    assert!(
        command
            .args
            .iter()
            .any(|arg| arg == "--config.global-bin-dir=/custom/pnpm-home"),
        "args: {:?}",
        command.args
    );
}

#[test]
fn does_not_self_update_when_the_install_path_is_not_writable() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let (_prefix, package_dir) = create_npm_prefix_install("npm-readonly");
        let context = with_empty_path(context(
            &package_dir,
            &package_dir.join("dist/cli.js").display().to_string(),
        ));
        fs::set_permissions(&package_dir, fs::Permissions::from_mode(0o500)).unwrap();

        assert_eq!(get_self_update_command(&context, PACKAGE, None, None), None);
        assert!(
            get_self_update_unavailable_instruction(&context, PACKAGE, None, None)
                .contains("the install path is not writable")
        );
        fs::set_permissions(&package_dir, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn the_rust_binary_behaves_like_the_compiled_typescript_binary() {
    let context = InstallContext::for_process().unwrap();
    assert_eq!(detect_install_method(&context), InstallMethod::BunBinary);
    assert_eq!(get_self_update_command(&context, PACKAGE, None, None), None);
    assert_eq!(
        get_update_instruction(&context, PACKAGE),
        "Download from: https://github.com/earendil-works/pi-mono/releases/latest"
    );
    assert_eq!(
        get_self_update_unavailable_instruction(&context, PACKAGE, None, None),
        "Download from: https://github.com/earendil-works/pi-mono/releases/latest"
    );
}

#[test]
fn detects_the_bun_runtime_and_npm_layouts_from_the_runtime_facts() {
    let package_dir = temp_dir("layout");
    let mut bun_runtime = with_empty_path(context(&package_dir, "/usr/local/bin/bun"));
    bun_runtime.is_bun_runtime = true;
    assert_eq!(detect_install_method(&bun_runtime), InstallMethod::Bun);

    let global_modules = with_empty_path(context(
        &package_dir,
        "/usr/local/lib/install/global/node_modules/pi/cli.js",
    ));
    assert_eq!(detect_install_method(&global_modules), InstallMethod::Bun);

    let npm = with_empty_path(context(&package_dir, "/usr/local/lib/node_modules/pi/cli.js"));
    assert_eq!(detect_install_method(&npm), InstallMethod::Npm);

    let yarn = with_empty_path(context(&package_dir, "/usr/local/lib/.yarn/pi/cli.js"));
    assert_eq!(detect_install_method(&yarn), InstallMethod::Yarn);

    let pnpm = with_empty_path(context(&package_dir, "/usr/local/lib/.pnpm/pi/cli.js"));
    assert_eq!(detect_install_method(&pnpm), InstallMethod::Pnpm);
}
