//! Ports `packages/coding-agent/test/initial-message.test.ts`.

use pi_ai::types::ImageContent;
use pi_coding_agent::cli::args::Args;
use pi_coding_agent::cli::initial_message::{InitialMessageInput, build_initial_message};

fn create_args(messages: &[&str]) -> Args {
    Args {
        messages: messages.iter().map(|message| (*message).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn merges_piped_stdin_with_the_first_cli_message_into_one_prompt() {
    let mut parsed = create_args(&["Summarize the text given"]);
    let result = build_initial_message(InitialMessageInput {
        parsed: &mut parsed,
        file_text: None,
        file_images: None,
        stdin_content: Some("README contents\n"),
    });

    assert_eq!(
        result.initial_message.as_deref(),
        Some("README contents\nSummarize the text given")
    );
    assert!(parsed.messages.is_empty());
    assert_eq!(result.initial_images, None);
}

#[test]
fn uses_stdin_as_the_initial_prompt_when_no_cli_message_is_present() {
    let mut parsed = create_args(&[]);
    let result = build_initial_message(InitialMessageInput {
        parsed: &mut parsed,
        file_text: None,
        file_images: None,
        stdin_content: Some("README contents"),
    });

    assert_eq!(result.initial_message.as_deref(), Some("README contents"));
    assert!(parsed.messages.is_empty());
}

#[test]
fn combines_stdin_file_text_and_first_cli_message_in_one_prompt() {
    let mut parsed = create_args(&["Explain it", "Second message"]);
    let result = build_initial_message(InitialMessageInput {
        parsed: &mut parsed,
        stdin_content: Some("stdin\n"),
        file_text: Some("file\n"),
        file_images: None,
    });

    assert_eq!(result.initial_message.as_deref(), Some("stdin\nfile\nExplain it"));
    assert_eq!(parsed.messages, vec!["Second message".to_owned()]);
}

#[test]
fn empty_inputs_produce_no_message_and_keep_images_optional() {
    let mut parsed = create_args(&[]);
    let result = build_initial_message(InitialMessageInput {
        parsed: &mut parsed,
        file_text: Some(""),
        file_images: None,
        stdin_content: None,
    });
    assert_eq!(result.initial_message, None);
    assert_eq!(result.initial_images, None);

    let mut parsed = create_args(&["First"]);
    let image = ImageContent::new("data", "image/png");
    let images = vec![image.clone()];
    let result = build_initial_message(InitialMessageInput {
        parsed: &mut parsed,
        file_text: None,
        file_images: Some(&images),
        stdin_content: None,
    });
    assert_eq!(result.initial_images, Some(images));
}
