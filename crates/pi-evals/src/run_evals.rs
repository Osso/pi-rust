//! Native entry point for `packages/evals/scripts/run-evals.mjs`.
//!
//! The child program is always `cargo`, resolved through PATH.

use std::fs::DirBuilder;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use pi_ai::utils::uuid::random_uuid;
use pi_js_compat::date::to_iso_string;
use pi_js_compat::node_fs::node_fs_error_record;
use pi_js_compat::string::js_trim;

const PACKAGE_ROOT: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Default)]
struct Arguments {
    provider: Option<String>,
    model: Option<String>,
    has_cli_selection: bool,
    forwarded: Vec<String>,
}

fn parse_arguments(args: &[String]) -> Result<Arguments, String> {
    let mut parsed = Arguments::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--provider" | "--model" => {
                let value = args.next().filter(|value| !value.is_empty() && !value.starts_with('-'));
                let value = value.ok_or_else(|| format!("Missing value for {arg}"))?;
                parsed.select(arg == "--provider", value);
            }
            _ if arg.starts_with("--provider=") => parsed.select(true, &arg[11..]),
            _ if arg.starts_with("--model=") => parsed.select(false, &arg[8..]),
            _ => parsed.forwarded.push(arg.clone()),
        }
    }
    Ok(parsed)
}

impl Arguments {
    fn select(&mut self, provider: bool, value: &str) {
        let field = if provider { &mut self.provider } else { &mut self.model };
        *field = Some(value.to_owned());
        self.has_cli_selection = true;
    }
}

fn trim_selection(value: Option<String>) -> Option<String> {
    value
        .map(|value| js_trim(&value).to_owned())
        .filter(|value| !value.is_empty())
}

fn resolve_selection(parsed: &Arguments, environment: [Option<String>; 2]) -> Result<Option<(String, String)>, String> {
    let [provider, model] = if parsed.has_cli_selection {
        [parsed.provider.clone(), parsed.model.clone()]
    } else {
        environment
    };
    match (trim_selection(provider), trim_selection(model)) {
        (Some(provider), Some(model)) => Ok(Some((provider, model))),
        (None, None) if !parsed.has_cli_selection => Ok(None),
        _ if parsed.has_cli_selection => Err("CLI model selection requires both --provider and --model.".into()),
        _ => Err("Default model selection requires both PI_PROVIDER and PI_MODEL.".into()),
    }
}

// Node's path.resolve is lexical: it does not require paths to exist or follow symlinks.
fn resolve_artifact_path(value: &Path) -> PathBuf {
    let path = Path::new(PACKAGE_ROOT).join(value);
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            _ => resolved.push(component.as_os_str()),
        }
    }
    resolved
}

fn read_artifact_directory() -> PathBuf {
    if let Some(value) = std::env::var_os("PI_EVAL_ARTIFACT_DIR").filter(|value| !value.is_empty()) {
        return resolve_artifact_path(Path::new(&value));
    }
    let millis = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis() as i64,
        Err(before_epoch) => -(before_epoch.duration().as_millis() as i64),
    };
    let timestamp = to_iso_string(millis).replace(':', "-");
    Path::new(PACKAGE_ROOT)
        .join(".eval")
        .join(format!("{timestamp}_{}", random_uuid()))
}

fn create_artifact_directory(path: &Path) -> Result<(), String> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| node_fs_error_record(&error, "mkdir", &path.to_string_lossy()).console_message(None))
}

fn configure_child(args: Vec<String>, artifacts: &Path, selection: Option<(String, String)>) -> Command {
    let mut child = Command::new("cargo");
    child
        .args(["test", "-p", "pi-evals"])
        .args(args)
        .current_dir(PACKAGE_ROOT)
        .env("PI_EVAL_ARTIFACT_DIR", artifacts)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some((provider, model)) = selection {
        child.env("PI_PROVIDER", provider).env("PI_MODEL", model);
    } else {
        child.env_remove("PI_PROVIDER").env_remove("PI_MODEL");
    }
    child
}

/// Validate selection, create artifacts, announce the run, and wait for the child.
///
/// Child processes inherit the caller's environment and standard streams.
pub fn run(args: &[String]) -> Result<i32, String> {
    let parsed = parse_arguments(args)?;
    let environment = [std::env::var("PI_PROVIDER").ok(), std::env::var("PI_MODEL").ok()];
    let selection = resolve_selection(&parsed, environment)?;
    let artifacts = read_artifact_directory();
    create_artifact_directory(&artifacts)?;
    let label = selection
        .as_ref()
        .map(|(provider, model)| format!("{provider}/{model}"))
        .unwrap_or_else(|| "none".into());
    eprintln!("[eval] default-model={label}");
    eprintln!("[eval] artifacts={}", artifacts.display());
    let mut child = configure_child(parsed.forwarded, &artifacts, selection);
    let status = child.status().map_err(|error| {
        let mut record = node_fs_error_record(&error, "spawnSync cargo", "cargo");
        record.message = format!("spawnSync cargo {}", record.code);
        let args: Vec<String> = child.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect();
        record.console_message(Some(&args))
    })?;
    Ok(status.code().unwrap_or(1))
}
