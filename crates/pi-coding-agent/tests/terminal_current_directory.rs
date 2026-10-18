use pi_coding_agent::utils::terminal_current_directory::format_terminal_current_directory_sequence;

fn hostname() -> String {
    gethostname::gethostname().to_string_lossy().into_owned()
}

#[test]
fn formats_an_osc_7_file_uri_with_local_hostname() {
    assert_eq!(
        format_terminal_current_directory_sequence("/syncthing/Sync/Projects/wow/wow-ui-sim"),
        format!(
            "\x1b]7;file://{}/syncthing/Sync/Projects/wow/wow-ui-sim\x1b\\",
            hostname()
        )
    );
}

#[test]
fn percent_encodes_characters_invalid_in_file_uris() {
    assert_eq!(
        format_terminal_current_directory_sequence("/tmp/project with spaces/#branch"),
        format!(
            "\x1b]7;file://{}/tmp/project%20with%20spaces/%23branch\x1b\\",
            hostname()
        )
    );
}

#[test]
fn percent_encodes_non_ascii_and_percent_signs() {
    assert_eq!(
        format_terminal_current_directory_sequence("/tmp/caf\u{e9} 100%/x~y"),
        format!("\x1b]7;file://{}/tmp/caf%C3%A9%20100%25/x%7Ey\x1b\\", hostname())
    );
}

#[test]
fn keeps_file_uri_punctuation_and_normalizes_dot_segments() {
    assert_eq!(
        format_terminal_current_directory_sequence("/tmp/a/../b:@!$&'()*+,;="),
        format!("\x1b]7;file://{}/tmp/b:@!$&'()*+,;=\x1b\\", hostname())
    );
}
