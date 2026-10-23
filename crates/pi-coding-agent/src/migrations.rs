//! One-time startup migrations (`migrations.ts`).
//!
//! Every migration takes an explicit agent directory in addition to the
//! TypeScript-shaped no-argument wrapper so tests and the differential oracle can
//! share one fixture tree without mutating process environment.
//!
//! Gap: the TTY keypress wait in [`show_deprecation_warnings`] needs a raw-mode
//! terminal. The non-TTY path is ported; the raw-mode read is deferred until the
//! interactive terminal is wired into the binary (see traceability).

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config;
use crate::core::output_guard::write_raw_stdout;
use crate::utils::chalk::Chalk;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};

pub const MIGRATION_GUIDE_URL: &str =
    "https://github.com/earendil-works/pi-mono/blob/main/packages/coding-agent/CHANGELOG.md#extensions-migration";
pub const EXTENSIONS_DOC_URL: &str =
    "https://github.com/earendil-works/pi-mono/blob/main/packages/coding-agent/docs/extensions.md";

/// Result of [`run_migrations`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationResult {
    pub migrated_auth_providers: Vec<JsString>,
    pub deprecation_warnings: Vec<JsString>,
}

/// Destination for migration progress lines (`console.log`).
pub trait MigrationOutput {
    fn line(&self, text: &JsString);
}

/// Writes progress lines through the raw stdout guard.
pub struct StdoutOutput;

impl MigrationOutput for StdoutOutput {
    fn line(&self, text: &JsString) {
        write_console_line(text);
    }
}

/// `console.log(text)`: stdout is the UTF-8 boundary.
fn write_console_line(text: &JsString) {
    write_raw_stdout(&format!("{}\n", encode_node_utf8(text)));
}

/// `readFileSync(path, "utf-8")` followed by `JSON.parse`.
fn read_json_file(path: &Path) -> Result<JsValue, ()> {
    let content = fs::read(path).map_err(drop)?;
    js_json::parse_lossless(String::from_utf8_lossy(&content).as_ref()).map_err(drop)
}

/// `readJsonRecord`: a parsed non-record becomes `{}`.
fn read_json_record(path: &Path) -> Result<JsObject, ()> {
    Ok(match read_json_file(path)? {
        JsValue::Object(record) => record,
        _ => JsObject::new(),
    })
}

fn read_auth_json(auth_path: &Path) -> Option<JsObject> {
    if !auth_path.exists() {
        return Some(JsObject::new());
    }
    read_json_record(auth_path).ok()
}

fn write_file_with_mode(path: &Path, contents: &str, mode: Option<u32>) -> std::io::Result<()> {
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(path)?;
        file.write_all(contents.as_bytes())?;
        return Ok(());
    }
    let _ = mode;
    fs::write(path, contents)
}

/// Migrate legacy `oauth.json` and `settings.json` `apiKeys` to `auth.json`.
pub fn migrate_auth_to_auth_json() -> Result<Vec<JsString>, config::ConfigError> {
    Ok(migrate_auth_to_auth_json_in(Path::new(&encode_node_utf8(
        &config::get_agent_dir()?,
    ))))
}

pub fn migrate_auth_to_auth_json_in(agent_dir: &Path) -> Vec<JsString> {
    let auth_path = agent_dir.join("auth.json");
    let oauth_path = agent_dir.join("oauth.json");
    let settings_path = agent_dir.join("settings.json");

    let Some(mut migrated) = read_auth_json(&auth_path) else {
        return Vec::new();
    };
    let mut providers = Vec::new();

    if oauth_path.exists()
        && let Ok(oauth) = read_json_record(&oauth_path)
    {
        for (provider, cred) in oauth {
            let JsValue::Object(cred) = cred else {
                continue;
            };
            if migrated.contains_key(&provider) {
                continue;
            }
            let mut entry = JsObject::new();
            entry.insert("type".into(), JsValue::String("oauth".into()));
            entry.extend(cred);
            migrated.insert(provider.clone(), JsValue::Object(entry));
            providers.push(provider);
        }
        let migrated_path = PathBuf::from(format!("{}.migrated", oauth_path.display()));
        let _ = fs::rename(&oauth_path, migrated_path);
    }

    if settings_path.exists()
        && let Ok(mut settings) = read_json_record(&settings_path)
        && let Some(JsValue::Object(api_keys)) = settings.get("apiKeys").cloned()
    {
        for (provider, key) in api_keys {
            let JsValue::String(key) = key else {
                continue;
            };
            if migrated.contains_key(&provider) {
                continue;
            }
            let mut entry = JsObject::new();
            entry.insert("type".into(), JsValue::String("api_key".into()));
            entry.insert("key".into(), JsValue::String(key));
            migrated.insert(provider.clone(), JsValue::Object(entry));
            providers.push(provider);
        }
        settings.shift_remove("apiKeys");
        let _ = write_file_with_mode(
            &settings_path,
            &js_json::stringify_lossless_pretty(&JsValue::Object(settings)),
            None,
        );
    }

    if !providers.is_empty() {
        if let Some(parent) = auth_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = write_file_with_mode(
            &auth_path,
            &js_json::stringify_lossless_pretty(&JsValue::Object(migrated)),
            Some(0o600),
        );
    }

    providers
}

/// `--${cwd.replace(/^[/\\]/, "").replace(/[/\\:]/g, "-")}--`, as `session-manager.ts` names session directories.
pub fn encode_session_directory_name(cwd: &JsString) -> JsString {
    let units = cwd.units();
    let stripped = match units.first() {
        Some(&unit) if unit == u16::from(b'/') || unit == u16::from(b'\\') => &units[1..],
        _ => units,
    };
    let replaced = stripped
        .iter()
        .map(|&unit| match unit {
            0x2f | 0x5c | 0x3a => u16::from(b'-'),
            other => other,
        })
        .collect();
    js_format!("--{}--", JsString::from_utf16(replaced))
}

/// `file.split("/").pop() || file.split("\\").pop()`.
fn js_file_name(file: &str) -> &str {
    let first = file.rsplit('/').next().unwrap_or("");
    if !first.is_empty() {
        return first;
    }
    file.rsplit('\\').next().unwrap_or("")
}

/// Session header `cwd` when the first line is a `session` header with a truthy string cwd.
/// A truthy non-string cwd throws in the source (`cwd.replace`) and the file is skipped.
fn session_header_cwd(content: &str) -> Option<JsString> {
    let first_line = content.split('\n').next().unwrap_or("");
    if JsString::from(first_line).trim().is_empty() {
        return None;
    }
    let JsValue::Object(header) = js_json::parse_lossless(first_line).ok()? else {
        return None;
    };
    if !matches!(header.get("type"), Some(JsValue::String(kind)) if kind == "session") {
        return None;
    }
    match header.get("cwd") {
        Some(JsValue::String(cwd)) if !cwd.is_empty() => Some(cwd.clone()),
        _ => None,
    }
}

/// Move `~/.pi/agent/*.jsonl` into the proper session directories.
pub fn migrate_sessions_from_agent_root() -> Result<(), config::ConfigError> {
    migrate_sessions_from_agent_root_in(Path::new(&encode_node_utf8(&config::get_agent_dir()?)));
    Ok(())
}

pub fn migrate_sessions_from_agent_root_in(agent_dir: &Path) {
    let Ok(entries) = fs::read_dir(agent_dir) else {
        return;
    };
    let files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|entry| agent_dir.join(entry.file_name()))
        .collect();

    for file in files {
        let Ok(content) = fs::read(&file) else {
            continue;
        };
        let Some(cwd) = session_header_cwd(&String::from_utf8_lossy(&content)) else {
            continue;
        };

        // Node encodes the directory name to UTF-8 at the fs boundary.
        let correct_dir = agent_dir
            .join("sessions")
            .join(encode_node_utf8(&encode_session_directory_name(&cwd)));
        if !correct_dir.exists() && fs::create_dir_all(&correct_dir).is_err() {
            continue;
        }

        let file_path = file.to_string_lossy().into_owned();
        let new_path = correct_dir.join(js_file_name(&file_path));
        if new_path.exists() {
            continue;
        }
        let _ = fs::rename(&file, &new_path);
    }
}

fn migrate_commands_to_prompts(base_dir: &Path, label: &str, output: &dyn MigrationOutput) -> bool {
    let chalk = Chalk::stdout();
    let commands_dir = base_dir.join("commands");
    let prompts_dir = base_dir.join("prompts");
    if !(commands_dir.exists() && !prompts_dir.exists()) {
        return false;
    }
    match fs::rename(&commands_dir, &prompts_dir) {
        Ok(()) => {
            output.line(&chalk.green(js_format!("Migrated {} commands/ → prompts/", label)));
            true
        }
        Err(error) => {
            output.line(&chalk.yellow(js_format!(
                "Warning: Could not migrate {} commands/ to prompts/: {}",
                label,
                error.to_string()
            )));
            false
        }
    }
}

fn migrate_keybindings_config_file_in(agent_dir: &Path) {
    let config_path = agent_dir.join("keybindings.json");
    if !config_path.exists() {
        return;
    }
    let Ok(JsValue::Object(parsed)) = read_json_file(&config_path) else {
        return;
    };
    let migration = crate::core::keybindings::migrate_keybindings_config(&parsed);
    if !migration.migrated {
        return;
    }
    let _ = fs::write(
        &config_path,
        format!(
            "{}\n",
            js_json::stringify_lossless_pretty(&JsValue::Object(migration.config))
        ),
    );
}

/// Move `fd`/`rg` binaries from `tools/` to `bin/`.
pub fn migrate_tools_to_bin() -> Result<(), config::ConfigError> {
    migrate_tools_to_bin_in(
        Path::new(&encode_node_utf8(&config::get_agent_dir()?)),
        Path::new(&encode_node_utf8(&config::get_bin_dir()?)),
    );
    Ok(())
}

pub fn migrate_tools_to_bin_in(agent_dir: &Path, bin_dir: &Path) {
    migrate_tools_to_bin_with(agent_dir, bin_dir, &StdoutOutput);
}

pub fn migrate_tools_to_bin_with(agent_dir: &Path, bin_dir: &Path, output: &dyn MigrationOutput) {
    let chalk = Chalk::stdout();
    let tools_dir = agent_dir.join("tools");
    if !tools_dir.exists() {
        return;
    }

    let mut moved_any = false;
    for binary in ["fd", "rg", "fd.exe", "rg.exe"] {
        let old_path = tools_dir.join(binary);
        let new_path = bin_dir.join(binary);
        if !old_path.exists() {
            continue;
        }
        if !bin_dir.exists() {
            let _ = fs::create_dir_all(bin_dir);
        }
        if !new_path.exists() {
            if fs::rename(&old_path, &new_path).is_ok() {
                moved_any = true;
            }
        } else {
            let _ = fs::remove_file(&old_path);
        }
    }

    if moved_any {
        output.line(&chalk.green("Migrated managed binaries tools/ → bin/"));
    }
}

fn check_deprecated_extension_dirs(base_dir: &Path, label: &str) -> Vec<JsString> {
    let hooks_dir = base_dir.join("hooks");
    let tools_dir = base_dir.join("tools");
    let mut warnings = Vec::new();

    if hooks_dir.exists() {
        warnings.push(js_format!(
            "{} hooks/ directory found. Hooks have been renamed to extensions.",
            label
        ));
    }

    if tools_dir.exists()
        && let Ok(entries) = fs::read_dir(&tools_dir)
    {
        let custom_tools = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let lower = name.to_lowercase();
                !matches!(lower.as_str(), "fd" | "rg" | "fd.exe" | "rg.exe") && !name.starts_with('.')
            })
            .count();
        if custom_tools > 0 {
            warnings.push(js_format!(
                "{} tools/ directory contains custom tools. Custom tools have been merged into extensions.",
                label
            ));
        }
    }

    warnings
}

fn migrate_extension_system(cwd: &JsString, agent_dir: &Path, output: &dyn MigrationOutput) -> Vec<JsString> {
    // `join(cwd, CONFIG_DIR_NAME)`; Node encodes the cwd to UTF-8 at the fs boundary.
    let project_dir = PathBuf::from(encode_node_utf8(&crate::utils::paths::node_join(
        cwd,
        &config::package_config().config_dir_name,
    )));
    migrate_commands_to_prompts(agent_dir, "Global", output);
    migrate_commands_to_prompts(&project_dir, "Project", output);

    let mut warnings = check_deprecated_extension_dirs(agent_dir, "Global");
    warnings.extend(check_deprecated_extension_dirs(&project_dir, "Project"));
    warnings
}

/// Print deprecation warnings and wait for a keypress on a TTY.
pub async fn show_deprecation_warnings(warnings: &[JsString]) {
    if warnings.is_empty() {
        return;
    }
    let chalk = Chalk::stdout();
    for warning in warnings {
        write_console_line(&chalk.yellow(js_format!("Warning: {}", warning)));
    }
    write_console_line(&chalk.yellow("\nMove your extensions to the extensions/ directory."));
    write_console_line(&chalk.yellow(js_format!("Migration guide: {}", MIGRATION_GUIDE_URL)));
    write_console_line(&chalk.yellow(js_format!("Documentation: {}", EXTENSIONS_DOC_URL)));

    wait_for_deprecation_keypress().await;
    write_raw_stdout("\n");
}

async fn wait_for_deprecation_keypress() {
    if !std::io::stdin().is_terminal() {
        return;
    }
    write_console_line(&Chalk::stdout().dim("\nPress any key to continue..."));
    // Gap: raw-mode keypress read requires the interactive terminal; the startup
    // binary does not wire one into this module yet.
}

/// Run all migrations. Called once on startup.
pub fn run_migrations(cwd: &JsString) -> Result<MigrationResult, config::ConfigError> {
    Ok(run_migrations_in(
        Path::new(&encode_node_utf8(&config::get_agent_dir()?)),
        Path::new(&encode_node_utf8(&config::get_bin_dir()?)),
        cwd,
    ))
}

pub fn run_migrations_in(agent_dir: &Path, bin_dir: &Path, cwd: &JsString) -> MigrationResult {
    run_migrations_with(agent_dir, bin_dir, cwd, &StdoutOutput)
}

pub fn run_migrations_with(
    agent_dir: &Path,
    bin_dir: &Path,
    cwd: &JsString,
    output: &dyn MigrationOutput,
) -> MigrationResult {
    let migrated_auth_providers = migrate_auth_to_auth_json_in(agent_dir);
    migrate_sessions_from_agent_root_in(agent_dir);
    migrate_tools_to_bin_with(agent_dir, bin_dir, output);
    migrate_keybindings_config_file_in(agent_dir);
    let deprecation_warnings = migrate_extension_system(cwd, agent_dir, output);
    MigrationResult {
        migrated_auth_providers,
        deprecation_warnings,
    }
}
