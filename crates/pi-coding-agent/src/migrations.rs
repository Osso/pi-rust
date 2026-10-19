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

use serde_json::{Map, Value};

use crate::config;
use crate::core::output_guard::write_raw_stdout;
use crate::utils::chalk::Chalk;
use pi_js_compat::js_json;

pub const MIGRATION_GUIDE_URL: &str =
    "https://github.com/earendil-works/pi-mono/blob/main/packages/coding-agent/CHANGELOG.md#extensions-migration";
pub const EXTENSIONS_DOC_URL: &str =
    "https://github.com/earendil-works/pi-mono/blob/main/packages/coding-agent/docs/extensions.md";

/// Result of [`run_migrations`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationResult {
    pub migrated_auth_providers: Vec<String>,
    pub deprecation_warnings: Vec<String>,
}

/// Destination for migration progress lines.
pub trait MigrationOutput {
    fn line(&self, text: &str);
}

/// Writes progress lines through the raw stdout guard.
pub struct StdoutOutput;

impl MigrationOutput for StdoutOutput {
    fn line(&self, text: &str) {
        write_raw_stdout(&format!("{text}\n"));
    }
}

fn is_record(value: &Value) -> bool {
    value.is_object()
}

fn read_json_record(path: &Path) -> Result<Map<String, Value>, String> {
    let content = fs::read(path).map_err(|error| error.to_string())?;
    let parsed = js_json::parse(&String::from_utf8_lossy(&content)).map_err(|error| error.to_string())?;
    Ok(match parsed {
        Value::Object(record) => record,
        _ => Map::new(),
    })
}

fn read_auth_json(auth_path: &Path) -> Option<Map<String, Value>> {
    if !auth_path.exists() {
        return Some(Map::new());
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
pub fn migrate_auth_to_auth_json() -> Vec<String> {
    migrate_auth_to_auth_json_in(&config::get_agent_dir())
}

pub fn migrate_auth_to_auth_json_in(agent_dir: &Path) -> Vec<String> {
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
            if migrated.contains_key(&provider) || !is_record(&cred) {
                continue;
            }
            let mut entry = Map::new();
            entry.insert("type".to_owned(), Value::String("oauth".to_owned()));
            if let Value::Object(cred) = cred {
                for (key, value) in cred {
                    entry.insert(key, value);
                }
            }
            migrated.insert(provider.clone(), Value::Object(entry));
            providers.push(provider);
        }
        let migrated_path = PathBuf::from(format!("{}.migrated", oauth_path.display()));
        let _ = fs::rename(&oauth_path, migrated_path);
    }

    if settings_path.exists()
        && let Ok(mut settings) = read_json_record(&settings_path)
    {
        let api_keys = settings.get("apiKeys").cloned();
        if let Some(Value::Object(api_keys)) = api_keys {
            for (provider, key) in api_keys {
                let Value::String(key) = key else {
                    continue;
                };
                if migrated.contains_key(&provider) {
                    continue;
                }
                let mut entry = Map::new();
                entry.insert("type".to_owned(), Value::String("api_key".to_owned()));
                entry.insert("key".to_owned(), Value::String(key));
                migrated.insert(provider.clone(), Value::Object(entry));
                providers.push(provider);
            }
            settings.remove("apiKeys");
            let _ = write_file_with_mode(
                &settings_path,
                &js_json::stringify_pretty(&Value::Object(settings)),
                None,
            );
        }
    }

    if !providers.is_empty() {
        if let Some(parent) = auth_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = write_file_with_mode(
            &auth_path,
            &js_json::stringify_pretty(&Value::Object(migrated)),
            Some(0o600),
        );
    }

    providers
}

/// Encode a session cwd the way `session-manager.ts` names session directories.
pub fn encode_session_directory_name(cwd: &str) -> String {
    let stripped = cwd.strip_prefix('/').or_else(|| cwd.strip_prefix('\\')).unwrap_or(cwd);
    let replaced: String = stripped
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other => other,
        })
        .collect();
    format!("--{replaced}--")
}

fn js_file_name(file: &str) -> String {
    let first = file.rsplit('/').next().unwrap_or("");
    if !first.is_empty() {
        return first.to_owned();
    }
    file.rsplit('\\').next().unwrap_or("").to_owned()
}

/// Move `~/.pi/agent/*.jsonl` into the proper session directories.
pub fn migrate_sessions_from_agent_root() {
    migrate_sessions_from_agent_root_in(&config::get_agent_dir());
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
    if files.is_empty() {
        return;
    }

    for file in files {
        let Ok(content) = fs::read(&file) else {
            continue;
        };
        let content = String::from_utf8_lossy(&content).into_owned();
        let first_line = content.split('\n').next().unwrap_or("");
        if first_line.trim().is_empty() {
            continue;
        }
        let Ok(header) = js_json::parse(first_line) else {
            continue;
        };
        let Value::Object(header) = header else {
            continue;
        };
        if header.get("type").and_then(Value::as_str) != Some("session") {
            continue;
        }
        let Some(Value::String(cwd)) = header.get("cwd") else {
            continue;
        };
        if cwd.is_empty() {
            continue;
        }

        let correct_dir = agent_dir.join("sessions").join(encode_session_directory_name(cwd));
        if !correct_dir.exists() {
            let _ = fs::create_dir_all(&correct_dir);
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
            output.line(&chalk.green(&format!("Migrated {label} commands/ → prompts/")));
            true
        }
        Err(error) => {
            output.line(&chalk.yellow(&format!(
                "Warning: Could not migrate {label} commands/ to prompts/: {error}"
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
    let Ok(content) = fs::read(&config_path) else {
        return;
    };
    let Ok(Value::Object(parsed)) = js_json::parse(&String::from_utf8_lossy(&content)) else {
        return;
    };
    let migration = crate::core::keybindings::migrate_keybindings_config(&parsed);
    if !migration.migrated {
        return;
    }
    let _ = fs::write(
        &config_path,
        format!("{}\n", js_json::stringify_pretty(&Value::Object(migration.config))),
    );
}

/// Move `fd`/`rg` binaries from `tools/` to `bin/`.
pub fn migrate_tools_to_bin() {
    migrate_tools_to_bin_in(&config::get_agent_dir(), &config::get_bin_dir());
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

fn check_deprecated_extension_dirs(base_dir: &Path, label: &str) -> Vec<String> {
    let hooks_dir = base_dir.join("hooks");
    let tools_dir = base_dir.join("tools");
    let mut warnings = Vec::new();

    if hooks_dir.exists() {
        warnings.push(format!(
            "{label} hooks/ directory found. Hooks have been renamed to extensions."
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
            warnings.push(format!(
                "{label} tools/ directory contains custom tools. Custom tools have been merged into extensions."
            ));
        }
    }

    warnings
}

fn migrate_extension_system(cwd: &str, agent_dir: &Path, output: &dyn MigrationOutput) -> Vec<String> {
    let project_dir = Path::new(cwd).join(&config::package_config().config_dir_name);
    migrate_commands_to_prompts(agent_dir, "Global", output);
    migrate_commands_to_prompts(&project_dir, "Project", output);

    let mut warnings = check_deprecated_extension_dirs(agent_dir, "Global");
    warnings.extend(check_deprecated_extension_dirs(&project_dir, "Project"));
    warnings
}

/// Print deprecation warnings and wait for a keypress on a TTY.
pub async fn show_deprecation_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    let chalk = Chalk::stdout();
    for warning in warnings {
        write_raw_stdout(&format!("{}\n", chalk.yellow(&format!("Warning: {warning}"))));
    }
    write_raw_stdout(&format!(
        "{}\n",
        chalk.yellow("\nMove your extensions to the extensions/ directory.")
    ));
    write_raw_stdout(&format!(
        "{}\n",
        chalk.yellow(&format!("Migration guide: {MIGRATION_GUIDE_URL}"))
    ));
    write_raw_stdout(&format!(
        "{}\n",
        chalk.yellow(&format!("Documentation: {EXTENSIONS_DOC_URL}"))
    ));

    wait_for_deprecation_keypress().await;
    write_raw_stdout("\n");
}

async fn wait_for_deprecation_keypress() {
    if !std::io::stdin().is_terminal() {
        return;
    }
    write_raw_stdout(&format!("{}\n", Chalk::stdout().dim("\nPress any key to continue...")));
    // Gap: raw-mode keypress read requires the interactive terminal; the startup
    // binary does not wire one into this module yet.
}

/// Run all migrations. Called once on startup.
pub fn run_migrations(cwd: &str) -> MigrationResult {
    run_migrations_in(&config::get_agent_dir(), &config::get_bin_dir(), cwd)
}

pub fn run_migrations_in(agent_dir: &Path, bin_dir: &Path, cwd: &str) -> MigrationResult {
    run_migrations_with(agent_dir, bin_dir, cwd, &StdoutOutput)
}

pub fn run_migrations_with(
    agent_dir: &Path,
    bin_dir: &Path,
    cwd: &str,
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
