use pi_coding_agent::utils::open_browser::{browser_command, launch_browser_command};

#[test]
fn selects_the_platform_launcher() {
    assert_eq!(
        browser_command("darwin", "https://example.com"),
        ("open".to_owned(), vec!["https://example.com".to_owned()])
    );
    assert_eq!(
        browser_command("win32", "https://example.com"),
        (
            "rundll32".to_owned(),
            vec![
                "url.dll,FileProtocolHandler".to_owned(),
                "https://example.com".to_owned()
            ]
        )
    );
    assert_eq!(
        browser_command("linux", "https://example.com"),
        ("xdg-open".to_owned(), vec!["https://example.com".to_owned()])
    );
}

#[test]
fn keeps_metacharacters_as_a_single_argument() {
    let (program, arguments) = browser_command("linux", "https://example.com/?a=1&b=2|3;4^5");
    assert_eq!(program, "xdg-open");
    assert_eq!(arguments, vec!["https://example.com/?a=1&b=2|3;4^5".to_owned()]);
}

#[test]
fn launcher_failures_do_not_panic() {
    launch_browser_command("pi-definitely-missing-launcher", &[]);
}
