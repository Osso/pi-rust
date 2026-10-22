//! Post-pin regressions mirrored from TS `editor-wide-wrap.test.ts`.
//! Each case runs in a subprocess: the unfixed stack overflow aborts only the
//! child, giving the parent test harness an observable failure.
use pi_js_compat::js_json::JsString;
use pi_tui::components::editor::{TextSegment, word_wrap_line};
use std::process::Command;

#[path = "support/editor_host.rs"]
mod editor_host;

fn assert_in_child(name: &str, check: impl FnOnce()) {
    const CHILD: &str = "PI_EDITOR_WIDE_WRAP_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        check();
        return;
    }
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, name)
        .output()
        .expect("spawn regression child");
    assert!(
        output.status.success(),
        "{name}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn overwide_graphemes_overflow_alone_and_preserve_chunk_offsets() {
    assert_in_child("overwide_graphemes_overflow_alone_and_preserve_chunk_offsets", || {
        for (line, expected) in [
            ("🚀", vec!["🚀"]),
            ("a🚀b", vec!["a", "🚀", "b"]),
            ("🚀🚀", vec!["🚀", "🚀"]),
            ("界a", vec!["界", "a"]),
            ("👩‍🚀x", vec!["👩‍🚀", "x"]),
        ] {
            let chunks = word_wrap_line(&JsString::from(line), 1, None);
            let texts: Vec<_> = chunks.iter().map(|chunk| chunk.text.clone()).collect();
            assert_eq!(texts, expected);
            let mut offset = 0;
            for chunk in chunks {
                assert_eq!(chunk.start_index, offset);
                offset += chunk.text.len();
                assert_eq!(chunk.end_index, offset);
                assert_eq!(
                    JsString::from(line).slice(chunk.start_index, chunk.end_index),
                    chunk.text
                );
            }
            assert_eq!(offset, JsString::from(line).len());
        }
    });
}

#[test]
fn merged_visual_segments_still_split_into_graphemes_at_width_one() {
    assert_in_child("merged_visual_segments_still_split_into_graphemes_at_width_one", || {
        let line = "a🚀b";
        let segments = [TextSegment {
            text: line.into(),
            index: 0,
        }];
        assert_eq!(
            word_wrap_line(&JsString::from(line), 1, Some(&segments)),
            word_wrap_line(&JsString::from(line), 1, None)
        );
    });
}

#[test]
fn narrow_rendering_and_visual_navigation_agree_on_overwide_chunks() {
    assert_in_child(
        "narrow_rendering_and_visual_navigation_agree_on_overwide_chunks",
        || {
            let (mut editor, _) = editor_host::editor();
            editor.set_text("a🚀b");
            let rendered = editor_host::render_plain(&mut editor, 2);
            assert_eq!(&rendered[1..rendered.len() - 1], ["a ", "🚀", "b "]);
            assert_eq!(editor.get_cursor(), (0, 4));
            editor_host::send(&mut editor, "\x1b[A");
            assert_eq!(editor.get_cursor(), (0, 1));
            let rendered = editor_host::render_plain(&mut editor, 2);
            assert_eq!(&rendered[1..rendered.len() - 1], ["a ", "🚀", "b "]);
            editor_host::send(&mut editor, "\x1b[A");
            assert_eq!(editor.get_cursor(), (0, 0));
            editor_host::send(&mut editor, "\x1b[B");
            assert_eq!(editor.get_cursor(), (0, 1));
            editor_host::send(&mut editor, "\x1b[B");
            assert_eq!(editor.get_cursor(), (0, 4));
        },
    );
}
