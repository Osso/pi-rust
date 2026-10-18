//! Behavioral coverage for the full `edit-diff.ts` port.

use pi_agent_core::harness::tools::edit_diff::{
    Edit, apply_edits_to_normalized_content, fuzzy_find_text, generate_diff_string, generate_unified_patch,
    normalize_for_fuzzy_match,
};

fn edit(old_text: &str, new_text: &str) -> Edit {
    Edit {
        old_text: old_text.to_owned(),
        new_text: new_text.to_owned(),
    }
}

#[test]
fn normalizes_fuzzy_input_progressively() {
    assert_eq!(normalize_for_fuzzy_match("a  \nb\t\n"), "a\nb\n");
    assert_eq!(
        normalize_for_fuzzy_match("\u{2018}x\u{2019} \u{201c}y\u{201d}"),
        "'x' \"y\""
    );
    assert_eq!(
        normalize_for_fuzzy_match("a\u{2010}b\u{2011}c\u{2012}d\u{2013}e\u{2014}f"),
        "a-b-c-d-e-f"
    );
    assert_eq!(normalize_for_fuzzy_match("a\u{00a0}b\u{2002}c\u{3000}d"), "a b c d");
    assert_eq!(normalize_for_fuzzy_match("\u{2460}"), "1");
}

#[test]
fn finds_exact_then_fuzzy_matches_with_utf16_offsets() {
    let exact = fuzzy_find_text("alpha beta", "beta");
    assert!(exact.found);
    assert_eq!(exact.index, 6.0);
    assert_eq!(exact.match_length, 4.0);
    assert!(!exact.used_fuzzy_match);
    assert_eq!(exact.content_for_replacement, "alpha beta");

    let supplementary = fuzzy_find_text("a\u{1f600}b", "b");
    assert_eq!(supplementary.index, 3.0);
    assert_eq!(supplementary.match_length, 1.0);

    let fuzzy = fuzzy_find_text("say \u{201c}hi\u{201d}  \n", "say \"hi\"");
    assert!(fuzzy.found);
    assert!(fuzzy.used_fuzzy_match);
    assert_eq!(fuzzy.content_for_replacement, "say \"hi\"\n");

    let missing = fuzzy_find_text("abc", "z");
    assert!(!missing.found);
    assert_eq!(missing.index, -1.0);
    assert_eq!(missing.match_length, 0.0);
    assert!(!missing.used_fuzzy_match);
    assert_eq!(missing.content_for_replacement, "abc");
}

#[test]
fn applies_single_and_disjoint_edits() {
    let single = apply_edits_to_normalized_content("one\ntwo\nthree\n", &[edit("two", "TWO")], "file.txt").unwrap();
    assert_eq!(single.base_content, "one\ntwo\nthree\n");
    assert_eq!(single.new_content, "one\nTWO\nthree\n");

    let disjoint = apply_edits_to_normalized_content(
        "alpha\nbeta\ngamma\ndelta\n",
        &[edit("alpha", "ALPHA"), edit("gamma", "GAMMA")],
        "file.txt",
    )
    .unwrap();
    assert_eq!(disjoint.new_content, "ALPHA\nbeta\nGAMMA\ndelta\n");
}

#[test]
fn preserves_unchanged_original_lines_during_fuzzy_edits() {
    let original = "alpha  \n\u{2018}beta\u{2019}\ngamma\n";
    let result = apply_edits_to_normalized_content(original, &[edit("'beta'", "'BETA'")], "file.txt").unwrap();
    assert_eq!(result.base_content, original);
    assert_eq!(result.new_content, "alpha  \n'BETA'\ngamma\n");
}

#[test]
fn rejects_empty_missing_duplicate_overlapping_and_no_change_edits() {
    let empty = apply_edits_to_normalized_content("abc", &[edit("", "x")], "f").unwrap_err();
    assert_eq!(empty.message, "oldText must not be empty in f.");
    let empty_multi = apply_edits_to_normalized_content("abc", &[edit("a", "A"), edit("", "x")], "f").unwrap_err();
    assert_eq!(empty_multi.message, "edits[1].oldText must not be empty in f.");

    let missing = apply_edits_to_normalized_content("abc", &[edit("z", "x")], "f").unwrap_err();
    assert_eq!(
        missing.message,
        "Could not find the exact text in f. The old text must match exactly including all whitespace and newlines."
    );
    let missing_multi = apply_edits_to_normalized_content("abc", &[edit("a", "A"), edit("z", "x")], "f").unwrap_err();
    assert_eq!(
        missing_multi.message,
        "Could not find edits[1] in f. The oldText must match exactly including all whitespace and newlines."
    );

    let duplicate = apply_edits_to_normalized_content("foo foo foo", &[edit("foo", "bar")], "f").unwrap_err();
    assert_eq!(
        duplicate.message,
        "Found 3 occurrences of the text in f. The text must be unique. Please provide more context to make it unique."
    );
    let duplicate_multi =
        apply_edits_to_normalized_content("foo foo", &[edit("foo", "bar"), edit("x", "y")], "f").unwrap_err();
    assert_eq!(
        duplicate_multi.message,
        "Found 2 occurrences of edits[0] in f. Each oldText must be unique. Please provide more context to make it unique."
    );

    let overlap = apply_edits_to_normalized_content(
        "one\ntwo\nthree\n",
        &[edit("one\ntwo\n", "A"), edit("two\nthree\n", "B")],
        "f",
    )
    .unwrap_err();
    assert_eq!(
        overlap.message,
        "edits[0] and edits[1] overlap in f. Merge them into one edit or target disjoint regions."
    );

    let no_change = apply_edits_to_normalized_content("abc", &[edit("abc", "abc")], "f").unwrap_err();
    assert_eq!(
        no_change.message,
        "No changes made to f. The replacement produced identical content. This might indicate an issue with special characters or the text not existing as expected."
    );
    let no_change_multi = apply_edits_to_normalized_content("abc", &[edit("a", "a"), edit("b", "b")], "f").unwrap_err();
    assert_eq!(
        no_change_multi.message,
        "No changes made to f. The replacements produced identical content."
    );
}

#[test]
fn generates_line_numbered_diffs_with_skipped_context() {
    let result = generate_diff_string("a\nb\nc\n", "a\nB\nc\n", 4);
    assert_eq!(result.diff, " 1 a\n-2 b\n+2 B\n 3 c");
    assert_eq!(result.first_changed_line, Some(2));

    let wide_old = (1..=20)
        .map(|line| format!("line-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut wide_new = wide_old.clone();
    wide_new = wide_new.replace("line-10", "LINE-10");
    let wide = generate_diff_string(&wide_old, &wide_new, 2);
    assert!(wide.diff.contains(" ..."));
    assert!(wide.diff.contains("-10 line-10"));
    assert!(wide.diff.contains("+10 LINE-10"));
    assert_eq!(wide.first_changed_line, Some(10));
}

#[test]
fn generates_unified_patches_with_file_headers_only() {
    let patch = generate_unified_patch("file.txt", "a\nb\nc\n", "a\nB\nc\n", 4);
    assert_eq!(patch, "--- file.txt\n+++ file.txt\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n");

    let empty_source = generate_unified_patch("file.txt", "", "x\n", 4);
    assert_eq!(empty_source, "--- file.txt\n+++ file.txt\n@@ -0,0 +1,1 @@\n+x\n");

    let no_final_newline = generate_unified_patch("file.txt", "a\nb", "a\nB", 4);
    assert_eq!(
        no_final_newline,
        "--- file.txt\n+++ file.txt\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+B\n\\ No newline at end of file\n"
    );
}

#[test]
fn applies_edits_across_line_endings_after_normalization() {
    let normalized = "one\ntwo\n";
    let result = apply_edits_to_normalized_content(normalized, &[edit("two\r\n", "TWO\r\n")], "f").unwrap();
    assert_eq!(result.new_content, "one\nTWO\n");
}
