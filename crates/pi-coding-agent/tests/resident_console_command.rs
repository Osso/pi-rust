//! `cli/resident-console-command.ts` argument parsing.

use pi_coding_agent::cli::resident_console_command::{ResidentConsoleCommand, parse_resident_console_args};
use pi_coding_agent::core::resident_console_transport::ResidentConsoleService;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn non_console_commands_are_not_handled() {
    assert_eq!(parse_resident_console_args(&[]), Ok(None));
    assert_eq!(parse_resident_console_args(&args(&["--help"])), Ok(None));
    assert_eq!(parse_resident_console_args(&args(&["--supervisors"])), Ok(None));
}

#[test]
fn service_flags_map_to_their_services() {
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor"])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Supervisor,
            initial_prompt: None,
        }))
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--architect"])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Architect,
            initial_prompt: None,
        }))
    );
}

#[test]
fn prompt_parts_are_joined_and_trimmed() {
    assert_eq!(
        parse_resident_console_args(&args(&["--architect", "  hello", "world  "])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Architect,
            initial_prompt: Some("hello world".to_owned()),
        }))
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor", "   "])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Supervisor,
            initial_prompt: None,
        }))
    );
}

#[test]
fn normal_cli_flags_are_rejected_with_the_service_name() {
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor", "prompt", "--model", "x"])),
        Err("--supervisor cannot be combined with normal Pi CLI flags".to_owned())
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--architect", "-l"])),
        Err("--architect cannot be combined with normal Pi CLI flags".to_owned())
    );
}
