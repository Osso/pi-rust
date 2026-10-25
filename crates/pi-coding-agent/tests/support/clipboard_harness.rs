#![allow(dead_code)]
//! Runs clipboard scenarios in a child copy of the test binary so the code
//! under test sees a PATH containing only fake clipboard tools, exactly like
//! the TypeScript tests mock `child_process`.

use crate::shared_executable_script as executable_script;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Output};

use pi_coding_agent::utils::clipboard_image::ClipboardEnvironment;
use tempfile::TempDir;

pub const SCENARIO_ENV: &str = "PI_CLIPBOARD_SCENARIO";
const BIN_ENV: &str = "PI_CLIPBOARD_FAKE_BIN";
const STATE_ENV: &str = "PI_CLIPBOARD_FAKE_STATE";
const PLATFORM_ENV: &str = "PI_CLIPBOARD_FAKE_PLATFORM";

/// Builds a Linux clipboard environment from the supplied variables.
pub fn environment(variables: &[(&str, &str)]) -> ClipboardEnvironment {
    platform_environment("linux", variables)
}

/// Builds a macOS clipboard environment from the supplied variables.
pub fn darwin_environment() -> ClipboardEnvironment {
    platform_environment("darwin", &[])
}

pub fn platform_environment(platform: &str, variables: &[(&str, &str)]) -> ClipboardEnvironment {
    ClipboardEnvironment::new(
        platform,
        variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).into()))
            .collect(),
    )
}

/// Minimal 1x1 24bpp red BMP, matching the clipboard test fixture.
pub fn bmp_1x1_red_24bpp() -> Vec<u8> {
    let mut buffer = vec![0u8; 58];
    buffer[0..2].copy_from_slice(b"BM");
    buffer[2..6].copy_from_slice(&(58u32).to_le_bytes());
    buffer[10..14].copy_from_slice(&54u32.to_le_bytes());
    buffer[14..18].copy_from_slice(&40u32.to_le_bytes());
    buffer[18..22].copy_from_slice(&1i32.to_le_bytes());
    buffer[22..26].copy_from_slice(&1i32.to_le_bytes());
    buffer[26..28].copy_from_slice(&1u16.to_le_bytes());
    buffer[28..30].copy_from_slice(&24u16.to_le_bytes());
    buffer[34..38].copy_from_slice(&4u32.to_le_bytes());
    buffer[56] = 0xff;
    buffer
}

/// Returns the fake environment when this process is the re-executed child.
pub fn child_environment(scenario: &str) -> Option<ChildEnvironment> {
    if std::env::var(SCENARIO_ENV).ok()? != scenario {
        return None;
    }
    let bin = PathBuf::from(std::env::var(BIN_ENV).expect("fake bin dir"));
    let state = PathBuf::from(std::env::var(STATE_ENV).expect("fake state dir"));
    let variables = std::env::vars()
        .map(|(name, value)| (name, value.into()))
        .collect::<BTreeMap<_, _>>();
    let platform = std::env::var(PLATFORM_ENV).expect("fake platform");
    Some(ChildEnvironment {
        environment: ClipboardEnvironment::new(platform, variables),
        bin,
        state,
    })
}

pub struct ChildEnvironment {
    pub environment: ClipboardEnvironment,
    pub bin: PathBuf,
    pub state: PathBuf,
}

impl ChildEnvironment {
    /// Records every invocation of a fake tool, one argv line per call.
    pub fn invocations(&self, tool: &str) -> Vec<String> {
        std::fs::read_to_string(self.state.join(format!("{tool}.log")))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

/// Fake clipboard tools keyed by executable name; each value is the script body.
pub type Scripts<'a> = &'a [(&'a str, &'a str)];

pub struct Scenario {
    _directory: TempDir,
    pub bin: PathBuf,
    pub state: PathBuf,
}

impl Scenario {
    pub fn new(scripts: Scripts<'_>) -> Self {
        let directory = tempfile::tempdir().expect("scenario directory");
        let bin = directory.path().join("bin");
        let state = directory.path().join("state");
        std::fs::create_dir_all(&bin).expect("fake bin directory");
        std::fs::create_dir_all(&state).expect("fake state directory");

        for (name, body) in scripts {
            let path = bin.join(name);
            executable_script::write_executable_script(&path, &format!("#!/bin/sh\n{body}"));
        }

        Scenario {
            _directory: directory,
            bin,
            state,
        }
    }
}

/// Runs `test_name` in a child test binary with the scenario PATH and variables.
pub fn run_scenario(test_name: &str, scenario: &Scenario, platform: &str, variables: &[(&str, &str)]) -> Output {
    let executable = std::env::current_exe().expect("test binary path");
    let mut command = Command::new(executable);
    command
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, test_name)
        .env(BIN_ENV, &scenario.bin)
        .env(STATE_ENV, &scenario.state)
        .env(PLATFORM_ENV, platform)
        .env("PATH", &scenario.bin);

    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "WSL_DISTRO_NAME",
        "WSLENV",
        "TERMUX_VERSION",
        "SSH_CONNECTION",
        "SSH_CLIENT",
        "MOSH_CONNECTION",
    ] {
        command.env_remove(name);
    }
    for (name, value) in variables {
        command.env(name, value);
    }

    command.output().expect("child test binary must run")
}

pub fn assert_child_passed(output: &Output) {
    assert!(
        output.status.success(),
        "child scenario failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Extracts every OSC 52 clipboard sequence written to stdout.
pub fn osc52_sequences(stdout: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(stdout);
    text.match_indices("\u{1b}]52;c;")
        .map(|(start, _)| {
            let rest = &text[start..];
            let end = rest.find('\u{7}').map_or(rest.len(), |offset| offset + 1);
            rest[..end].to_owned()
        })
        .collect()
}

pub fn osc52_sequence(text: &str) -> String {
    format!(
        "\u{1b}]52;c;{}\u{7}",
        pi_js_compat::base64::buffer_to_base64(text.as_bytes())
    )
}

/// Renders bytes as a POSIX `printf` argument so fake tools can emit binary
/// payloads without external programs.
pub fn octal_printf(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("\\{byte:03o}")).collect()
}
