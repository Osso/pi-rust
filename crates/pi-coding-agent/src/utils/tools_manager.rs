//! PATH-only discovery from frozen `packages/coding-agent/src/utils/tools-manager.ts`.

use std::process::{Command, Stdio};

use pi_js_compat::js_json::JsString;

use crate::utils::chalk::Chalk;

/// The frozen module's `"fd" | "rg"` argument domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Fd,
    Rg,
}

struct ToolConfig {
    name: &'static str,
    commands: &'static [&'static str],
    termux_package: &'static str,
}

impl Tool {
    fn config(self) -> ToolConfig {
        match self {
            Self::Fd => ToolConfig {
                name: "fd",
                commands: &["fd", "fdfind"],
                termux_package: "fd",
            },
            Self::Rg => ToolConfig {
                name: "ripgrep",
                commands: &["rg"],
                termux_package: "ripgrep",
            },
        }
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .env_clear()
        .envs(pi_js_compat::process_env::vars())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .is_ok()
}

/// Return the first spawnable PATH command; a nonzero version exit still counts.
pub fn get_tool_path(tool: Tool) -> Option<JsString> {
    tool.config()
        .commands
        .iter()
        .find(|command| command_exists(command))
        .map(|command| JsString::from(*command))
}

/// Discover a tool without installation; optionally print the frozen install hint.
pub async fn ensure_tool(tool: Tool, silent: bool) -> Option<JsString> {
    if let Some(path) = get_tool_path(tool) {
        return Some(path);
    }
    if !silent {
        let message = missing_tool_message(tool, std::env::consts::OS);
        crate::main_runner::console_log(Chalk::stdout().yellow(message));
    }
    None
}

fn missing_tool_message(tool: Tool, platform: &str) -> String {
    let config = tool.config();
    let hint = if platform == "android" {
        format!("Install with: pkg install {}.", config.termux_package)
    } else {
        "Install it with your system package manager.".to_owned()
    };
    format!("{} not found. {hint}", config.name)
}

#[cfg(test)]
mod tests {
    use super::{Tool, missing_tool_message};

    #[test]
    fn android_install_hints_keep_the_frozen_package_and_punctuation() {
        assert_eq!(
            missing_tool_message(Tool::Fd, "android"),
            "fd not found. Install with: pkg install fd."
        );
        assert_eq!(
            missing_tool_message(Tool::Rg, "android"),
            "ripgrep not found. Install with: pkg install ripgrep."
        );
    }
}
