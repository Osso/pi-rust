use pi_protocol::{ModelRef, ModelRefError, SessionPhase, ThinkingLevel};

#[test]
fn thinking_levels_round_trip_exact_source_literals() {
    let cases = [
        ("off", ThinkingLevel::Off),
        ("minimal", ThinkingLevel::Minimal),
        ("low", ThinkingLevel::Low),
        ("medium", ThinkingLevel::Medium),
        ("high", ThinkingLevel::High),
        ("xhigh", ThinkingLevel::XHigh),
        ("max", ThinkingLevel::Max),
    ];

    for (literal, expected) in cases {
        let level = literal
            .parse::<ThinkingLevel>()
            .expect("source thinking-level literals must parse");
        assert_eq!(level, expected);
        assert_eq!(level.as_str(), literal);
    }
}

#[test]
fn thinking_levels_reject_non_literals() {
    for value in ["", "HIGH", "High", "ultra", "unknown", " high", "high ", "é", "𐅑"] {
        assert!(
            value.parse::<ThinkingLevel>().is_err(),
            "unexpectedly accepted thinking level {value:?}"
        );
    }
}

#[test]
fn session_phases_round_trip_exact_source_literals() {
    let cases = [
        ("idle", SessionPhase::Idle),
        ("turn", SessionPhase::Turn),
        ("compaction", SessionPhase::Compaction),
        ("branch_summary", SessionPhase::BranchSummary),
        ("retry", SessionPhase::Retry),
    ];

    for (literal, expected) in cases {
        let phase = literal
            .parse::<SessionPhase>()
            .expect("source session-phase literals must parse");
        assert_eq!(phase, expected);
        assert_eq!(phase.as_str(), literal);
    }
}

#[test]
fn session_phases_reject_non_literals() {
    for value in ["", "Idle", "branch-summary", "branch summary", " retry", "retry ", "é"] {
        assert!(
            value.parse::<SessionPhase>().is_err(),
            "unexpectedly accepted session phase {value:?}"
        );
    }
}

#[test]
fn model_ref_preserves_non_empty_strings_exactly() {
    let cases = [
        ("openai", "gpt-5"),
        ("é", "モデル"),
        ("𐅑", "e\u{301}"),
        (" provider ", " model "),
        ("   ", "\t\n"),
        ("provider\0name", "model\0id"),
    ];

    for (provider, id) in cases {
        let model_ref =
            ModelRef::new(provider.to_owned(), id.to_owned()).expect("non-empty provider and id must construct");

        assert_eq!(model_ref.provider(), provider);
        assert_eq!(model_ref.id(), id);
    }
}

#[test]
fn model_ref_rejects_empty_provider_before_empty_id() {
    assert!(matches!(
        ModelRef::new(String::new(), "model".to_owned()),
        Err(ModelRefError::EmptyProvider)
    ));
    assert!(matches!(
        ModelRef::new(String::new(), String::new()),
        Err(ModelRefError::EmptyProvider)
    ));
}

#[test]
fn model_ref_rejects_empty_id() {
    assert!(matches!(
        ModelRef::new("provider".to_owned(), String::new()),
        Err(ModelRefError::EmptyId)
    ));
}
