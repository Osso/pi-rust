use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_coding_agent::core::tools::truncate::{TruncatedBy, TruncationOptions, truncate_tail};

fn accumulator(options: OutputAccumulatorOptions) -> OutputAccumulator {
    OutputAccumulator::new(options)
}

#[test]
fn appends_utf8_across_chunk_boundaries() {
    let mut output = accumulator(OutputAccumulatorOptions::default());

    output.append(&[b'a', 0xE2, 0x82]);
    output.append(&[0xAC, b'b']);
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions::default());
    assert_eq!(snapshot.content, "a€b");
    assert_eq!(snapshot.truncation.total_bytes, 5);
    assert_eq!(snapshot.truncation.total_lines, 1);
    assert!(!snapshot.truncation.truncated);
    assert_eq!(snapshot.full_output_path, None);
}

#[test]
fn strips_a_leading_bom_across_chunk_boundaries_but_not_later() {
    let mut whole = accumulator(OutputAccumulatorOptions::default());
    whole.append(&[0xEF, 0xBB, 0xBF, b'a']);
    whole.finish();
    assert_eq!(whole.snapshot(SnapshotOptions::default()).content, "a");

    let mut split = accumulator(OutputAccumulatorOptions::default());
    split.append(&[0xEF]);
    split.append(&[0xBB]);
    split.append(&[0xBF]);
    split.finish();
    assert_eq!(split.snapshot(SnapshotOptions::default()).content, "");

    let mut later = accumulator(OutputAccumulatorOptions::default());
    later.append(&[b'A', 0xEF, 0xBB, 0xBF]);
    later.finish();
    assert_eq!(later.snapshot(SnapshotOptions::default()).content, "A\u{FEFF}");
}

#[test]
fn counts_lines_and_the_open_line_bytes() {
    let mut output = accumulator(OutputAccumulatorOptions::default());

    output.append(b"one\ntwo");
    assert_eq!(output.last_line_bytes(), 3);
    output.append(b"\nthree");
    assert_eq!(output.last_line_bytes(), 5);
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions::default());
    assert_eq!(snapshot.truncation.total_lines, 3);
    assert_eq!(snapshot.content, "one\ntwo\nthree");
}

#[test]
fn keeps_only_the_rolling_tail_for_snapshots() {
    let mut output = accumulator(OutputAccumulatorOptions {
        max_lines: 3,
        max_bytes: 16,
        ..OutputAccumulatorOptions::default()
    });
    output.append(b"line-0\nline-1\nline-2\nline-3\nline-4\nline-5");
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions::default());

    assert_eq!(snapshot.content, "line-4\nline-5");
    assert_eq!(snapshot.truncation.total_lines, 6);
    assert_eq!(snapshot.truncation.truncated_by, Some(TruncatedBy::Bytes));
    assert!(snapshot.truncation.truncated);
}

#[test]
fn snapshot_truncation_uses_tail_limits_and_full_counts() {
    let mut output = accumulator(OutputAccumulatorOptions {
        max_lines: 2,
        max_bytes: 1_024,
        ..OutputAccumulatorOptions::default()
    });
    output.append(b"a\nb\nc\n");
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions::default());
    let tail = truncate_tail(
        "a\nb\nc\n",
        TruncationOptions {
            max_lines: 2,
            max_bytes: 1_024,
        },
    );

    assert_eq!(snapshot.truncation.content, tail.content);
    assert_eq!(snapshot.truncation.total_lines, 3);
    assert_eq!(snapshot.truncation.output_lines, tail.output_lines);
}

#[test]
fn persists_truncated_output_to_the_configured_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.log");
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_lines: 2,
            max_bytes: 1_024,
            ..OutputAccumulatorOptions::default()
        },
        JsString::from(path.to_str().unwrap()),
    );
    output.append(b"a\nb\nc\n");
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });

    assert_eq!(
        snapshot.full_output_path.as_ref(),
        Some(path.to_str().unwrap()).map(JsString::from).as_ref()
    );
    assert_eq!(fs::read(&path).unwrap(), b"a\nb\nc\n");
}

#[test]
fn does_not_create_a_temp_file_when_the_snapshot_is_not_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.log");
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions::default(),
        JsString::from(path.to_str().unwrap()),
    );
    output.append(b"a\n");
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });

    assert_eq!(snapshot.full_output_path, None);
    assert!(!path.exists());
}

#[test]
fn persists_all_raw_chunks_once_the_output_exceeds_the_limits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.log");
    let mut output = OutputAccumulator::new_with_temp_file_path(
        OutputAccumulatorOptions {
            max_lines: 1,
            max_bytes: 1_024,
            ..OutputAccumulatorOptions::default()
        },
        JsString::from(path.to_str().unwrap()),
    );
    output.append(b"a\n");
    output.append(b"b\n");
    output.finish();

    let snapshot = output.snapshot(SnapshotOptions {
        persist_if_truncated: true,
    });

    assert_eq!(
        snapshot.full_output_path.as_ref(),
        Some(path.to_str().unwrap()).map(JsString::from).as_ref()
    );
    assert_eq!(fs::read(&path).unwrap(), b"a\nb\n");
}

#[test]
#[should_panic(expected = "Cannot append to a finished output accumulator")]
fn appending_after_finish_fails() {
    let mut output = accumulator(OutputAccumulatorOptions::default());
    output.finish();
    output.append(b"late");
}

#[test]
fn finish_is_idempotent_and_flushes_pending_bytes() {
    let mut output = accumulator(OutputAccumulatorOptions::default());
    output.append(&[0xE2, 0x82]);
    output.finish();
    output.finish();

    assert_eq!(output.snapshot(SnapshotOptions::default()).content, "\u{FFFD}");
}
