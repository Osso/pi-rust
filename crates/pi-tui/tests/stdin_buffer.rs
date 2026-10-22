use std::time::{Duration, Instant};

use pi_tui::stdin_buffer::{StdinBuffer, StdinBufferEvent, StdinBufferOptions};

fn buffer() -> StdinBuffer {
    StdinBuffer::with_options(StdinBufferOptions {
        timeout: Duration::from_millis(10),
    })
}

fn data(value: &str) -> StdinBufferEvent {
    StdinBufferEvent::Data(value.to_string())
}

fn paste(value: &str) -> StdinBufferEvent {
    StdinBufferEvent::Paste(value.to_string())
}

fn poll_later(buffer: &mut StdinBuffer) -> Vec<StdinBufferEvent> {
    buffer.poll(Instant::now() + Duration::from_secs(1))
}

#[test]
fn passes_regular_characters_immediately() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("a"), vec![data("a")]);
    assert_eq!(buffer.process("abc"), vec![data("a"), data("b"), data("c")]);
    assert_eq!(
        buffer.process("hello 世界"),
        vec![
            data("h"),
            data("e"),
            data("l"),
            data("l"),
            data("o"),
            data(" "),
            data("世"),
            data("界")
        ]
    );
}

#[test]
fn passes_complete_escape_sequences() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b[<35;20;5m"), vec![data("\x1b[<35;20;5m")]);
    assert_eq!(buffer.process("\x1b[A"), vec![data("\x1b[A")]);
    assert_eq!(buffer.process("\x1b[11~"), vec![data("\x1b[11~")]);
    assert_eq!(buffer.process("\x1ba"), vec![data("\x1ba")]);
    assert_eq!(buffer.process("\x1bOA"), vec![data("\x1bOA")]);
}

#[test]
fn buffers_incomplete_mouse_sequences_across_chunks() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b").is_empty());
    assert_eq!(buffer.buffer(), "\x1b");
    assert!(buffer.process("[<35").is_empty());
    assert_eq!(buffer.buffer(), "\x1b[<35");
    assert_eq!(buffer.process(";20;5m"), vec![data("\x1b[<35;20;5m")]);
    assert_eq!(buffer.buffer(), "");
}

#[test]
fn buffers_incomplete_csi_sequences() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[").is_empty());
    assert!(buffer.process("1;").is_empty());
    assert_eq!(buffer.process("5H"), vec![data("\x1b[1;5H")]);
}

#[test]
fn buffers_split_sequences_across_many_chunks() {
    let mut buffer = buffer();
    let mut events = Vec::new();
    for chunk in ["\x1b", "[", "<", "3", "5", ";", "2", "0", ";", "5", "m"] {
        events.extend(buffer.process(chunk));
    }
    assert_eq!(events, vec![data("\x1b[<35;20;5m")]);
}

#[test]
fn flushes_incomplete_sequences_after_timeout() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[<35").is_empty());
    assert_eq!(poll_later(&mut buffer), vec![data("\x1b[<35")]);
    assert_eq!(buffer.buffer(), "");
}

#[test]
fn flushes_a_lone_escape_after_the_escape_timeout() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b").is_empty());
    assert_eq!(poll_later(&mut buffer), vec![data("\x1b")]);
    assert_eq!(buffer.process("\r"), vec![data("\r")]);
}

#[test]
fn merges_escape_and_carriage_return_within_a_larger_timeout() {
    let mut buffer = StdinBuffer::with_options(StdinBufferOptions {
        timeout: Duration::from_millis(100),
    });
    assert!(buffer.process("\x1b").is_empty());
    assert!(buffer.poll(Instant::now() + Duration::from_millis(20)).is_empty());
    assert_eq!(buffer.process("\r"), vec![data("\x1b\r")]);
}

#[test]
fn applies_the_sequence_timeout_to_a_lone_escape() {
    let mut buffer = StdinBuffer::with_options(StdinBufferOptions {
        timeout: Duration::from_millis(100),
    });
    assert!(buffer.process("\x1b").is_empty());
    assert!(buffer.poll(Instant::now() + Duration::from_millis(20)).is_empty());
    assert_eq!(
        buffer.poll(Instant::now() + Duration::from_millis(110)),
        vec![data("\x1b")]
    );
}

#[test]
fn keeps_fragmented_mouse_sequences_buffered_with_default_timeouts() {
    let mut buffer = StdinBuffer::new();
    assert!(buffer.process("\x1b[").is_empty());
    assert!(buffer.poll(Instant::now() + Duration::from_millis(5)).is_empty());
    assert_eq!(buffer.process("<65;48;39M"), vec![data("\x1b[<65;48;39M")]);
}

#[test]
fn handles_mixed_content() {
    let mut buffer = buffer();
    assert_eq!(
        buffer.process("abc\x1b[A"),
        vec![data("a"), data("b"), data("c"), data("\x1b[A")]
    );
    assert_eq!(
        buffer.process("\x1b[Aabc"),
        vec![data("\x1b[A"), data("a"), data("b"), data("c")]
    );
    assert_eq!(
        buffer.process("\x1b[A\x1b[B\x1b[C"),
        vec![data("\x1b[A"), data("\x1b[B"), data("\x1b[C")]
    );
    assert_eq!(buffer.process("abc\x1b[<35"), vec![data("a"), data("b"), data("c")]);
    assert_eq!(buffer.buffer(), "\x1b[<35");
    assert_eq!(buffer.process(";20;5m"), vec![data("\x1b[<35;20;5m")]);
}

#[test]
fn handles_kitty_csi_u_press_release_and_batches() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b[97u"), vec![data("\x1b[97u")]);
    assert_eq!(buffer.process("\x1b[97;1:3u"), vec![data("\x1b[97;1:3u")]);
    assert_eq!(
        buffer.process("\x1b[97u\x1b[97;1:3u"),
        vec![data("\x1b[97u"), data("\x1b[97;1:3u")]
    );
    assert_eq!(
        buffer.process("\x1b[97u\x1b[97;1:3u\x1b[98u\x1b[98;1:3u"),
        vec![
            data("\x1b[97u"),
            data("\x1b[97;1:3u"),
            data("\x1b[98u"),
            data("\x1b[98;1:3u")
        ]
    );
    assert_eq!(buffer.process("\x1b[1;1:1A"), vec![data("\x1b[1;1:1A")]);
    assert_eq!(buffer.process("\x1b[3;1:3~"), vec![data("\x1b[3;1:3~")]);
}

#[test]
fn splits_esc_esc_csi_into_standalone_escape_and_csi() {
    let mut buffer = buffer();
    assert_eq!(
        buffer.process("\x1b\x1b[27;129:3u"),
        vec![data("\x1b"), data("\x1b[27;129:3u")]
    );
    assert_eq!(
        buffer.process("\x1b\x1b[27;1:3u"),
        vec![data("\x1b"), data("\x1b[27;1:3u")]
    );
}

#[test]
fn keeps_esc_esc_as_one_sequence_when_not_followed_by_an_escape() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b\x1b"), vec![data("\x1b\x1b")]);
}

#[test]
fn drops_raw_duplicate_characters_after_kitty_printable_sequences() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b[224uà"), vec![data("\x1b[224u")]);
    assert_eq!(buffer.process("\x1b[64u"), vec![data("\x1b[64u")]);
    assert_eq!(buffer.process("@"), Vec::<StdinBufferEvent>::new());
    assert_eq!(buffer.process("\x1b[97ub"), vec![data("\x1b[97u"), data("b")]);
    assert_eq!(buffer.process("\x1b[64;3u@"), vec![data("\x1b[64;3u"), data("@")]);
}

#[test]
fn handles_mouse_events() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b[<0;10;5M"), vec![data("\x1b[<0;10;5M")]);
    assert_eq!(buffer.process("\x1b[<0;10;5m"), vec![data("\x1b[<0;10;5m")]);
    assert_eq!(buffer.process("\x1b[<35;20;5m"), vec![data("\x1b[<35;20;5m")]);
    assert!(buffer.process("\x1b[<3").is_empty());
    assert!(buffer.process("5;1").is_empty());
    assert!(buffer.process("5;").is_empty());
    assert_eq!(buffer.process("10m"), vec![data("\x1b[<35;15;10m")]);
    assert_eq!(
        buffer.process("\x1b[<35;1;1m\x1b[<35;2;2m\x1b[<35;3;3m"),
        vec![data("\x1b[<35;1;1m"), data("\x1b[<35;2;2m"), data("\x1b[<35;3;3m")]
    );
}

#[test]
fn handles_old_style_mouse_sequences() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("\x1b[M abc"), vec![data("\x1b[M ab"), data("c")]);
    assert!(buffer.process("\x1b[M").is_empty());
    assert_eq!(buffer.buffer(), "\x1b[M");
    assert!(buffer.process(" a").is_empty());
    assert_eq!(buffer.buffer(), "\x1b[M a");
    assert_eq!(buffer.process("b"), vec![data("\x1b[M ab")]);
}

#[test]
fn handles_empty_input() {
    let mut buffer = buffer();
    assert_eq!(buffer.process(""), vec![data("")]);
}

#[test]
fn flushes_a_lone_escape_promptly_with_default_timeouts() {
    let mut buffer = StdinBuffer::new();
    assert!(buffer.process("\x1b").is_empty());
    assert_eq!(
        buffer.poll(Instant::now() + Duration::from_millis(20)),
        vec![data("\x1b")]
    );
}

#[test]
fn explicit_flush_returns_and_clears_buffered_sequences() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[<35").is_empty());
    assert_eq!(buffer.flush(), vec!["\x1b[<35".to_string()]);
    assert_eq!(buffer.buffer(), "");
    assert_eq!(buffer.flush(), Vec::<String>::new());
}

#[test]
fn clear_drops_buffered_content_without_emitting() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[<35").is_empty());
    buffer.clear();
    assert_eq!(buffer.buffer(), "");
    assert!(poll_later(&mut buffer).is_empty());
}

#[test]
fn handles_buffer_input_and_high_byte_conversion() {
    let mut buffer = buffer();
    assert_eq!(buffer.process_bytes(b"\x1b[A"), vec![data("\x1b[A")]);
    assert_eq!(buffer.process_bytes(&[200]), vec![data("\x1bH")]);
}

#[test]
fn handles_very_long_sequences() {
    let mut buffer = buffer();
    let long_sequence = format!("\x1b[{}H", "1;".repeat(50));
    assert_eq!(buffer.process(&long_sequence), vec![data(&long_sequence)]);
}

#[test]
fn handles_complete_bracketed_paste() {
    let mut buffer = buffer();
    assert_eq!(
        buffer.process("\x1b[200~hello world\x1b[201~"),
        vec![paste("hello world")]
    );
}

#[test]
fn handles_paste_arriving_in_chunks() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[200~").is_empty());
    assert!(buffer.process("hello ").is_empty());
    assert_eq!(buffer.process("world\x1b[201~"), vec![paste("hello world")]);
}

#[test]
fn handles_paste_with_input_before_and_after() {
    let mut buffer = buffer();
    assert_eq!(buffer.process("a"), vec![data("a")]);
    assert_eq!(buffer.process("\x1b[200~pasted\x1b[201~"), vec![paste("pasted")]);
    assert_eq!(buffer.process("b"), vec![data("b")]);
}

#[test]
fn handles_paste_with_newlines_and_unicode() {
    let mut buffer = buffer();
    assert_eq!(
        buffer.process("\x1b[200~line1\nline2\nline3\x1b[201~"),
        vec![paste("line1\nline2\nline3")]
    );
    assert_eq!(
        buffer.process("\x1b[200~Hello 世界 🎉\x1b[201~"),
        vec![paste("Hello 世界 🎉")]
    );
}

#[test]
fn destroy_clears_buffer_and_pending_timeout() {
    let mut buffer = buffer();
    assert!(buffer.process("\x1b[<35").is_empty());
    buffer.destroy();
    assert_eq!(buffer.buffer(), "");
    assert!(poll_later(&mut buffer).is_empty());
}
