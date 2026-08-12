use pi_protocol::{SessionMetadata, SessionMetadataError, SessionMetadataInput};

fn required_input() -> SessionMetadataInput {
    SessionMetadataInput {
        id: "session-id".to_owned(),
        created_at: 1_700_000_000,
        updated_at: None,
        parent_session_id: None,
        session_name: None,
        cwd: None,
    }
}

#[test]
fn required_fields_preserve_optional_absence() {
    let metadata = SessionMetadata::new(required_input()).expect("required fields must construct");

    assert_eq!(metadata.id(), "session-id");
    assert_eq!(metadata.created_at(), 1_700_000_000);
    assert_eq!(metadata.updated_at(), None);
    assert_eq!(metadata.parent_session_id(), None);
    assert_eq!(metadata.session_name(), None);
    assert_eq!(metadata.cwd(), None);
}

#[test]
fn preserves_all_optional_fields_exactly() {
    let metadata = SessionMetadata::new(SessionMetadataInput {
        id: "session-id".to_owned(),
        created_at: 10,
        updated_at: Some(20),
        parent_session_id: Some("parent-id".to_owned()),
        session_name: Some("named session".to_owned()),
        cwd: Some("/tmp/project".to_owned()),
    })
    .expect("complete metadata must construct");

    assert_eq!(metadata.updated_at(), Some(20));
    assert_eq!(metadata.parent_session_id(), Some("parent-id"));
    assert_eq!(metadata.session_name(), Some("named session"));
    assert_eq!(metadata.cwd(), Some("/tmp/project"));
}

#[test]
fn omission_differs_from_an_explicit_empty_session_name() {
    let omitted = SessionMetadata::new(required_input()).expect("omitted name must construct");
    let explicit_empty = SessionMetadata::new(SessionMetadataInput {
        session_name: Some(String::new()),
        ..required_input()
    })
    .expect("explicit empty name must construct");

    assert_eq!(omitted.session_name(), None);
    assert_eq!(explicit_empty.session_name(), Some(""));
}

#[test]
fn accepts_boundary_timestamps_and_representable_non_empty_strings() {
    let metadata = SessionMetadata::new(SessionMetadataInput {
        id: "e\u{301}".to_owned(),
        created_at: 0,
        updated_at: Some(u64::MAX),
        parent_session_id: Some("   ".to_owned()),
        session_name: Some(String::new()),
        cwd: Some("𐅑\0cwd".to_owned()),
    })
    .expect("representable non-empty strings and timestamp boundaries must construct");

    assert_eq!(metadata.id(), "e\u{301}");
    assert_eq!(metadata.created_at(), 0);
    assert_eq!(metadata.updated_at(), Some(u64::MAX));
    assert_eq!(metadata.parent_session_id(), Some("   "));
    assert_eq!(metadata.session_name(), Some(""));
    assert_eq!(metadata.cwd(), Some("𐅑\0cwd"));
}

#[test]
fn rejects_empty_fields_in_source_order() {
    assert!(matches!(
        SessionMetadata::new(SessionMetadataInput {
            id: String::new(),
            ..required_input()
        }),
        Err(SessionMetadataError::EmptyId)
    ));
    assert!(matches!(
        SessionMetadata::new(SessionMetadataInput {
            parent_session_id: Some(String::new()),
            ..required_input()
        }),
        Err(SessionMetadataError::EmptyParentSessionId)
    ));
    assert!(matches!(
        SessionMetadata::new(SessionMetadataInput {
            cwd: Some(String::new()),
            ..required_input()
        }),
        Err(SessionMetadataError::EmptyCwd)
    ));
    assert!(matches!(
        SessionMetadata::new(SessionMetadataInput {
            id: String::new(),
            parent_session_id: Some(String::new()),
            cwd: Some(String::new()),
            ..required_input()
        }),
        Err(SessionMetadataError::EmptyId)
    ));
}

#[test]
fn validation_errors_name_the_empty_cwd_field() {
    assert_eq!(
        SessionMetadataError::EmptyCwd.to_string(),
        "Session metadata field cwd must not be empty"
    );
}
