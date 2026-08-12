use std::error::Error as StdError;
use std::fmt::{self, Debug, Display, Formatter};

use pi_agent_core::harness::{BranchSummaryError, BranchSummaryErrorCode, CompactionError, CompactionErrorCode};

#[derive(Debug)]
struct TestCause {
    message: String,
}

impl Display for TestCause {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl StdError for TestCause {}

fn assert_error_traits<T: StdError + Display + Debug>(_: &T) {}

#[test]
fn distinct_code_enums_preserve_exact_literals() {
    let compaction_codes = [
        (CompactionErrorCode::Aborted, "aborted"),
        (CompactionErrorCode::SummarizationFailed, "summarization_failed"),
    ];
    let branch_summary_codes = [
        (BranchSummaryErrorCode::Aborted, "aborted"),
        (BranchSummaryErrorCode::SummarizationFailed, "summarization_failed"),
    ];

    for (code, literal) in compaction_codes {
        assert_eq!(code.as_str(), literal);
    }
    for (code, literal) in branch_summary_codes {
        assert_eq!(code.as_str(), literal);
    }
}

#[test]
fn minimal_compaction_error_preserves_fields_and_traits() {
    let error = CompactionError::new(CompactionErrorCode::Aborted, "compaction stopped", None);

    assert_eq!(error.name, "CompactionError");
    assert_eq!(error.message, "compaction stopped");
    assert_eq!(error.code, CompactionErrorCode::Aborted);
    assert!(error.cause.is_none());
    assert_eq!(error.to_string(), "compaction stopped");
    assert!(error.source().is_none());
    assert_error_traits(&error);
}

#[test]
fn minimal_branch_summary_error_preserves_fields_and_traits() {
    let error = BranchSummaryError::new(
        BranchSummaryErrorCode::SummarizationFailed,
        "branch summary failed",
        None,
    );

    assert_eq!(error.name, "BranchSummaryError");
    assert_eq!(error.message, "branch summary failed");
    assert_eq!(error.code, BranchSummaryErrorCode::SummarizationFailed);
    assert!(error.cause.is_none());
    assert_eq!(error.to_string(), "branch summary failed");
    assert!(error.source().is_none());
    assert_error_traits(&error);
}

#[test]
fn both_errors_preserve_required_string_messages() {
    let messages = ["", "描述🙂", "e\u{301}", "embedded\0nul", "punctuation!?.,:;()[]{}"];

    for message in messages {
        let compaction = CompactionError::new(CompactionErrorCode::Aborted, message, None);
        let branch_summary = BranchSummaryError::new(BranchSummaryErrorCode::Aborted, message, None);

        assert_eq!(compaction.message, message);
        assert_eq!(compaction.to_string(), message);
        assert_eq!(branch_summary.message, message);
        assert_eq!(branch_summary.to_string(), message);
    }
}

#[test]
fn both_errors_preserve_ordinary_typed_causes() {
    let compaction = CompactionError::new(
        CompactionErrorCode::SummarizationFailed,
        "compaction failed",
        Some(Box::new(TestCause {
            message: "compaction cause".to_owned(),
        })),
    );
    let branch_summary = BranchSummaryError::new(
        BranchSummaryErrorCode::SummarizationFailed,
        "branch summary failed",
        Some(Box::new(TestCause {
            message: "branch summary cause".to_owned(),
        })),
    );

    assert_eq!(
        compaction.source().map(ToString::to_string).as_deref(),
        Some("compaction cause")
    );
    assert_eq!(
        branch_summary.source().map(ToString::to_string).as_deref(),
        Some("branch summary cause")
    );
}

#[test]
fn cross_class_chains_preserve_both_directions() {
    let branch_summary = BranchSummaryError::new(
        BranchSummaryErrorCode::Aborted,
        "branch summary",
        Some(Box::new(TestCause {
            message: "branch root".to_owned(),
        })),
    );
    let compaction_outer = CompactionError::new(
        CompactionErrorCode::Aborted,
        "compaction outer",
        Some(Box::new(branch_summary)),
    );
    let branch_source = compaction_outer.source().expect("branch summary source");
    assert_eq!(branch_source.to_string(), "branch summary");
    assert_eq!(
        branch_source.source().map(ToString::to_string).as_deref(),
        Some("branch root")
    );

    let compaction = CompactionError::new(
        CompactionErrorCode::Aborted,
        "compaction",
        Some(Box::new(TestCause {
            message: "compaction root".to_owned(),
        })),
    );
    let branch_outer = BranchSummaryError::new(
        BranchSummaryErrorCode::Aborted,
        "branch outer",
        Some(Box::new(compaction)),
    );
    let compaction_source = branch_outer.source().expect("compaction source");
    assert_eq!(compaction_source.to_string(), "compaction");
    assert_eq!(
        compaction_source.source().map(ToString::to_string).as_deref(),
        Some("compaction root")
    );
}

#[test]
fn public_mutation_updates_display_and_source() {
    let mut compaction = CompactionError::new(CompactionErrorCode::Aborted, "before", None);
    compaction.name = "renamed compaction".to_owned();
    compaction.message = "after compaction".to_owned();
    compaction.code = CompactionErrorCode::SummarizationFailed;
    compaction.cause = Some(Box::new(TestCause {
        message: "updated compaction cause".to_owned(),
    }));

    assert_eq!(compaction.name, "renamed compaction");
    assert_eq!(compaction.message, "after compaction");
    assert_eq!(compaction.code, CompactionErrorCode::SummarizationFailed);
    assert_eq!(compaction.to_string(), "after compaction");
    assert_eq!(
        compaction.source().map(ToString::to_string).as_deref(),
        Some("updated compaction cause")
    );

    let mut branch_summary = BranchSummaryError::new(BranchSummaryErrorCode::Aborted, "before", None);
    branch_summary.name = "renamed branch".to_owned();
    branch_summary.message = "after branch".to_owned();
    branch_summary.code = BranchSummaryErrorCode::SummarizationFailed;
    branch_summary.cause = Some(Box::new(TestCause {
        message: "updated branch cause".to_owned(),
    }));

    assert_eq!(branch_summary.name, "renamed branch");
    assert_eq!(branch_summary.message, "after branch");
    assert_eq!(branch_summary.code, BranchSummaryErrorCode::SummarizationFailed);
    assert_eq!(branch_summary.to_string(), "after branch");
    assert_eq!(
        branch_summary.source().map(ToString::to_string).as_deref(),
        Some("updated branch cause")
    );
}
