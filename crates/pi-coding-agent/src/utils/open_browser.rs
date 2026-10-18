use std::process::{Command, Stdio};

use crate::utils::pi_user_agent::node_platform_name;

/// Select the platform browser launcher without invoking a shell.
pub fn browser_command(platform: &str, target: &str) -> (String, Vec<String>) {
    match platform {
        "darwin" => ("open".to_owned(), vec![target.to_owned()]),
        "win32" => (
            "rundll32".to_owned(),
            vec!["url.dll,FileProtocolHandler".to_owned(), target.to_owned()],
        ),
        _ => ("xdg-open".to_owned(), vec![target.to_owned()]),
    }
}

/// Open a URL or file in the platform browser/default handler.
///
/// Browser launch is best effort: launcher failures are ignored.
pub fn open_browser(target: &str) {
    let (program, arguments) = browser_command(node_platform_name(std::env::consts::OS), target);
    launch_browser_command(&program, &arguments);
}

/// Spawn a detached launcher with ignored stdio.
pub fn launch_browser_command(program: &str, arguments: &[String]) {
    let _ = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
