use std::fs;
use std::os::unix::fs::PermissionsExt;

use pi_coding_agent::core::tools::edit_diff::{
    Edit, EditDiffOutcome, apply_edits_to_normalized_content, compute_edit_diff, compute_edits_diff, fuzzy_find_text,
    generate_diff_string, generate_unified_patch, normalize_for_fuzzy_match, strip_bom,
};

fn edits(pairs: &[(&str, &str)]) -> Vec<Edit> {
    pairs
        .iter()
        .map(|(old_text, new_text)| Edit {
            old_text: (*old_text).to_owned(),
            new_text: (*new_text).to_owned(),
        })
        .collect()
}

#[test]
fn fuzzy_normalization_applies_nfkc_trim_end_quotes_dashes_and_spaces() {
    assert_eq!(
        normalize_for_fuzzy_match("“quoted”  \n‘single’\t\nem—dash − minus\u{00A0}space\nﬁle Ａ"),
        "\"quoted\"\n'single'\nem-dash - minus space\nfile A"
    );
    assert_eq!(normalize_for_fuzzy_match("e\u{0301}"), "é");
}

#[test]
fn fuzzy_find_prefers_exact_matches_and_reports_utf16_offsets() {
    let exact = fuzzy_find_text("a😀target", "target");
    assert!(exact.found);
    assert!(!exact.used_fuzzy_match);
    assert_eq!(exact.index, 3.0);
    assert_eq!(exact.match_length, 6.0);
    assert_eq!(exact.content_for_replacement, "a😀target");

    let fuzzy = fuzzy_find_text("say “hello”  \nnext", "say \"hello\"\nnext");
    assert!(fuzzy.found);
    assert!(fuzzy.used_fuzzy_match);
    assert_eq!(fuzzy.content_for_replacement, "say \"hello\"\nnext");

    let missing = fuzzy_find_text("abc", "xyz");
    assert!(!missing.found);
    assert_eq!(missing.index, -1.0);
    assert_eq!(missing.match_length, 0.0);
}

#[test]
fn strip_bom_is_reexported_for_the_coding_agent_module() {
    let stripped = strip_bom("\u{FEFF}hello");
    assert_eq!(stripped.bom, "\u{FEFF}");
    assert_eq!(stripped.text, "hello");
}

#[test]
fn applies_multiple_disjoint_edits_against_the_same_base() {
    let result = apply_edits_to_normalized_content(
        "alpha\nbeta\ngamma\n",
        &edits(&[("alpha", "ALPHA"), ("gamma", "GAMMA")]),
        "sample.txt",
    )
    .expect("disjoint edits apply");

    assert_eq!(result.base_content, "alpha\nbeta\ngamma\n");
    assert_eq!(result.new_content, "ALPHA\nbeta\nGAMMA\n");
}

#[test]
fn reports_missing_empty_overlapping_and_no_change_edits() {
    let missing = apply_edits_to_normalized_content("abc", &edits(&[("zzz", "x")]), "f.txt").unwrap_err();
    assert_eq!(
        missing.to_string(),
        "Could not find the exact text in f.txt. The old text must match exactly including all whitespace and newlines."
    );

    let missing_indexed =
        apply_edits_to_normalized_content("abc", &edits(&[("a", "A"), ("zzz", "x")]), "f.txt").unwrap_err();
    assert_eq!(
        missing_indexed.to_string(),
        "Could not find edits[1] in f.txt. The oldText must match exactly including all whitespace and newlines."
    );

    let empty = apply_edits_to_normalized_content("abc", &edits(&[("", "x")]), "f.txt").unwrap_err();
    assert_eq!(empty.to_string(), "oldText must not be empty in f.txt.");

    let empty_indexed =
        apply_edits_to_normalized_content("abc", &edits(&[("a", "A"), ("", "x")]), "f.txt").unwrap_err();
    assert_eq!(
        empty_indexed.to_string(),
        "edits[1].oldText must not be empty in f.txt."
    );

    let overlap =
        apply_edits_to_normalized_content("abcdef", &edits(&[("abcd", "x"), ("cdef", "y")]), "f.txt").unwrap_err();
    assert_eq!(
        overlap.to_string(),
        "edits[0] and edits[1] overlap in f.txt. Merge them into one edit or target disjoint regions."
    );

    let unchanged = apply_edits_to_normalized_content("abc", &edits(&[("b", "b")]), "f.txt").unwrap_err();
    assert_eq!(
        unchanged.to_string(),
        "No changes made to f.txt. The replacement produced identical content. This might indicate an issue with special characters or the text not existing as expected."
    );
    let unchanged_multi =
        apply_edits_to_normalized_content("abc", &edits(&[("a", "a"), ("c", "c")]), "f.txt").unwrap_err();
    assert_eq!(
        unchanged_multi.to_string(),
        "No changes made to f.txt. The replacements produced identical content."
    );
}

#[test]
fn duplicate_matches_report_line_numbers_and_context() {
    let content = [
        "function first() {",
        "\treturn value;",
        "}",
        "",
        "function second() {",
        "\treturn value;",
        "}",
    ]
    .join("\n");
    let error = apply_edits_to_normalized_content(
        &content,
        &edits(&[("\treturn value;", "\treturn changed;")]),
        "sample.ts",
    )
    .unwrap_err()
    .to_string();

    assert!(error.starts_with(
        "Found 2 occurrences of the text in sample.ts. The text must be unique. Please provide more context to make it unique."
    ));
    assert!(error.contains("Duplicate match context:"));
    assert!(error.contains("1) line 2\n  1: function first() {\n> 2: \treturn value;"));
    assert!(error.contains("2) line 6\n  4: \n  5: function second() {\n> 6: \treturn value;"));

    let multi = apply_edits_to_normalized_content(
        "alpha\ntarget\nbeta\ntarget\ngamma\n",
        &edits(&[("alpha", "ALPHA"), ("target", "TARGET")]),
        "sample.txt",
    )
    .unwrap_err()
    .to_string();
    assert!(multi.starts_with("Found 2 occurrences of edits[1] in sample.txt. Each oldText must be unique."));
    assert!(multi.contains("1) line 2"));
    assert!(multi.contains("2) line 4"));
}

#[test]
fn fuzzy_edits_preserve_unchanged_lines_from_the_original() {
    let original = "keep  \r\nsay “hi”  \nkeep2\n";
    let normalized = pi_coding_agent::core::tools::edit_diff::normalize_to_lf(original);
    let result = apply_edits_to_normalized_content(&normalized, &edits(&[("say \"hi\"", "say \"bye\"")]), "f.txt")
        .expect("fuzzy edit applies");

    assert_eq!(result.base_content, "keep  \nsay “hi”  \nkeep2\n");
    assert_eq!(result.new_content, "keep  \nsay \"bye\"\nkeep2\n");
}

#[test]
fn diff_string_numbers_lines_and_marks_the_first_changed_line() {
    let result = generate_diff_string("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n", "a\nb\nc\nd\ne\nf\nG\nh\ni\nj\n", 1);

    assert_eq!(result.diff, "    ...\n  6 f\n- 7 g\n+ 7 G\n  8 h\n    ...");
    assert_eq!(result.first_changed_line, Some(7));

    let addition_only = generate_diff_string("a\nb\n", "a\nnew\nb\n", 4);
    assert_eq!(addition_only.diff, " 1 a\n+2 new\n 2 b");
    assert_eq!(addition_only.first_changed_line, Some(2));
}

#[test]
fn diff_string_skips_long_context_runs_with_markers() {
    let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    let new = "1\n2\n3\n4\nX\n6\n7\n8\n9\n10\n";
    let result = generate_diff_string(old, new, 1);

    assert_eq!(result.diff, "    ...\n  4 4\n- 5 5\n+ 5 X\n  6 6\n    ...");
}

#[test]
fn unified_patch_uses_file_headers_and_no_newline_markers() {
    let patch = generate_unified_patch("file.txt", "a\nb", "a\nc", 4);

    assert_eq!(
        patch,
        "--- file.txt\n+++ file.txt\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n"
    );
}

#[tokio::test]
async fn computes_a_diff_preview_for_a_crlf_file_with_a_bom() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    fs::write(&path, "\u{FEFF}one\r\ntwo\r\n").unwrap();

    let outcome = compute_edits_diff(
        path.to_str().unwrap(),
        &edits(&[("two", "TWO")]),
        dir.path().to_str().unwrap(),
    )
    .await
    .unwrap();

    match outcome {
        EditDiffOutcome::Diff(diff) => {
            assert_eq!(diff.diff, " 1 one\n-2 two\n+2 TWO");
            assert_eq!(diff.first_changed_line, Some(2));
        }
        EditDiffOutcome::Error(error) => panic!("unexpected error: {error}"),
    }
}

#[tokio::test]
async fn reports_missing_unreadable_and_directory_targets() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let missing = dir.path().join("missing.txt");

    let missing_outcome = compute_edits_diff(missing.to_str().unwrap(), &edits(&[("a", "b")]), cwd)
        .await
        .unwrap();
    assert_eq!(
        missing_outcome,
        EditDiffOutcome::Error(format!(
            "Could not edit file: {}. Error code: ENOENT.",
            missing.display()
        ))
    );

    let unreadable = dir.path().join("unreadable.txt");
    fs::write(&unreadable, "hello\n").unwrap();
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o222)).unwrap();
    let unreadable_outcome = compute_edits_diff(unreadable.to_str().unwrap(), &edits(&[("a", "b")]), cwd)
        .await
        .unwrap();
    assert_eq!(
        unreadable_outcome,
        EditDiffOutcome::Error(format!(
            "Could not edit file: {}. Error code: EACCES.",
            unreadable.display()
        ))
    );

    let directory_outcome = compute_edits_diff(dir.path().to_str().unwrap(), &edits(&[("a", "b")]), cwd)
        .await
        .unwrap();
    assert_eq!(
        directory_outcome,
        EditDiffOutcome::Error(format!(
            "EISDIR: illegal operation on a directory, read '{}'",
            dir.path().display()
        ))
    );
}

#[tokio::test]
async fn single_edit_diff_wraps_the_multi_edit_form() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.txt");
    fs::write(&path, "hello\n").unwrap();

    let outcome = compute_edit_diff(path.to_str().unwrap(), "hello", "world", dir.path().to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(
        outcome,
        EditDiffOutcome::Diff(pi_coding_agent::core::tools::edit_diff::EditDiffResult {
            diff: "-1 hello\n+1 world".to_owned(),
            first_changed_line: Some(1),
        })
    );
}
