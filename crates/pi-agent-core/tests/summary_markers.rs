use pi_agent_core::harness::messages::{
    BRANCH_SUMMARY_PREFIX, BRANCH_SUMMARY_SUFFIX, COMPACTION_SUMMARY_PREFIX, COMPACTION_SUMMARY_SUFFIX,
};
use pi_agent_core::harness::{
    BRANCH_SUMMARY_PREFIX as ROOT_BRANCH_PREFIX, BRANCH_SUMMARY_SUFFIX as ROOT_BRANCH_SUFFIX,
    COMPACTION_SUMMARY_PREFIX as ROOT_COMPACTION_PREFIX, COMPACTION_SUMMARY_SUFFIX as ROOT_COMPACTION_SUFFIX,
};

const _: [(); 96] = [(); COMPACTION_SUMMARY_PREFIX.len()];
const _: [(); 11] = [(); COMPACTION_SUMMARY_SUFFIX.len()];
const _: [(); 89] = [(); BRANCH_SUMMARY_PREFIX.len()];
const _: [(); 10] = [(); BRANCH_SUMMARY_SUFFIX.len()];

#[test]
fn exposes_exact_marker_values_and_lengths() {
    assert_eq!(COMPACTION_SUMMARY_PREFIX.len(), 96);
    assert_eq!(COMPACTION_SUMMARY_SUFFIX.len(), 11);
    assert_eq!(BRANCH_SUMMARY_PREFIX.len(), 89);
    assert_eq!(BRANCH_SUMMARY_SUFFIX.len(), 10);

    assert_eq!(
        COMPACTION_SUMMARY_PREFIX,
        "The conversation history before this point was compacted into the following summary:\n\n<summary>\n"
    );
    assert_eq!(COMPACTION_SUMMARY_SUFFIX, "\n</summary>");
    assert_eq!(
        BRANCH_SUMMARY_PREFIX,
        "The following is a summary of a branch that this conversation came back from:\n\n<summary>\n"
    );
    assert_eq!(BRANCH_SUMMARY_SUFFIX, "</summary>");

    for marker in [
        COMPACTION_SUMMARY_PREFIX,
        COMPACTION_SUMMARY_SUFFIX,
        BRANCH_SUMMARY_PREFIX,
        BRANCH_SUMMARY_SUFFIX,
    ] {
        assert!(!marker.contains('\r'));
        assert!(!marker.ends_with(' '));
    }
}

#[test]
fn module_and_root_exports_are_the_same_const_values() {
    assert_eq!(COMPACTION_SUMMARY_PREFIX, ROOT_COMPACTION_PREFIX);
    assert_eq!(COMPACTION_SUMMARY_SUFFIX, ROOT_COMPACTION_SUFFIX);
    assert_eq!(BRANCH_SUMMARY_PREFIX, ROOT_BRANCH_PREFIX);
    assert_eq!(BRANCH_SUMMARY_SUFFIX, ROOT_BRANCH_SUFFIX);
}

#[test]
fn compaction_and_branch_framing_preserve_summary_content() {
    let summaries = [
        "",
        "plain summary",
        "前😀\0<&>\"'\r\nsecond",
        "<summary>\n</summary>\n<summary>\r\n",
    ];

    for summary in summaries {
        let compaction = format!("{COMPACTION_SUMMARY_PREFIX}{summary}{COMPACTION_SUMMARY_SUFFIX}");
        let branch = format!("{BRANCH_SUMMARY_PREFIX}{summary}{BRANCH_SUMMARY_SUFFIX}");

        assert_eq!(
            &compaction[COMPACTION_SUMMARY_PREFIX.len()..compaction.len() - COMPACTION_SUMMARY_SUFFIX.len()],
            summary
        );
        assert_eq!(
            &branch[BRANCH_SUMMARY_PREFIX.len()..branch.len() - BRANCH_SUMMARY_SUFFIX.len()],
            summary
        );
        assert!(compaction.starts_with(COMPACTION_SUMMARY_PREFIX));
        assert!(compaction.ends_with(COMPACTION_SUMMARY_SUFFIX));
        assert!(branch.starts_with(BRANCH_SUMMARY_PREFIX));
        assert!(branch.ends_with(BRANCH_SUMMARY_SUFFIX));
    }
}

#[test]
fn marker_constants_are_usable_in_const_bindings() {
    const COMPACTION_EMPTY: &str = COMPACTION_SUMMARY_PREFIX;
    const BRANCH_EMPTY: &str = BRANCH_SUMMARY_PREFIX;

    assert_eq!(COMPACTION_EMPTY, COMPACTION_SUMMARY_PREFIX);
    assert_eq!(BRANCH_EMPTY, BRANCH_SUMMARY_PREFIX);
}
