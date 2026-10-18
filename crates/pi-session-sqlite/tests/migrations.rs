//! Port of `packages/session-backends/sqlite-node/test/migrations.test.ts`.

mod support;

use pi_session_sqlite::{apply_migrations, open_database};
use support::Fixture;

fn column_names(db: &rusqlite::Connection, table: &str) -> Vec<String> {
    db.prepare(&format!("PRAGMA table_info({table})"))
        .expect("table info")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("table info rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("column names")
}

fn index_names(db: &rusqlite::Connection, table: &str) -> Vec<String> {
    db.prepare(&format!("PRAGMA index_list({table})"))
        .expect("index list")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("index rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("index names")
}

#[test]
fn applies_the_current_schema_once_and_records_its_migration() {
    let fixture = Fixture::new();
    let db = open_database(&fixture.database_path).expect("database");
    apply_migrations(&db).expect("first migration run");
    apply_migrations(&db).expect("second migration run");

    let ids: Vec<String> = db
        .prepare("SELECT id FROM migrations ORDER BY id")
        .expect("migration rows")
        .query_map([], |row| row.get(0))
        .expect("migration ids")
        .collect::<Result<Vec<_>, _>>()
        .expect("migration list");
    assert_eq!(ids, vec!["001_initial.sql".to_owned()]);

    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("table rows")
        .query_map([], |row| row.get(0))
        .expect("table names")
        .collect::<Result<Vec<_>, _>>()
        .expect("tables");
    for expected in [
        "migrations",
        "sessions",
        "entries",
        "session_sequences",
        "session_stats",
        "branch_entries",
        "branch_tips",
        "lanes",
        "records",
        "lane_moves",
        "facts",
        "writer_leases",
    ] {
        assert!(tables.iter().any(|table| table == expected), "missing table {expected}");
    }

    let session_columns = column_names(&db, "sessions");
    assert!(!session_columns.iter().any(|column| column == "leaf_id"));
    let session_indexes = index_names(&db, "sessions");
    assert!(
        session_indexes
            .iter()
            .any(|index| index == "idx_sessions_cwd_created_at")
    );
    assert!(!session_indexes.iter().any(|index| index == "idx_sessions_parent"));
    let lane_columns = column_names(&db, "lanes");
    assert!(lane_columns.iter().any(|column| column == "open_operation_id"));
    let entry_indexes = index_names(&db, "entries");
    assert!(!entry_indexes.iter().any(|index| index == "idx_entries_session_seq"));
    let branch_entry_indexes = index_names(&db, "branch_entries");
    assert!(
        branch_entry_indexes
            .iter()
            .any(|index| index == "idx_branch_entries_session_entry")
    );
    let record_indexes = index_names(&db, "records");
    for expected in [
        "idx_records_session_lane_seq",
        "idx_records_session_type_seq",
        "idx_records_session_type_op_kind_seq",
    ] {
        assert!(
            record_indexes.iter().any(|index| index == expected),
            "missing index {expected}"
        );
    }
    assert!(!record_indexes.iter().any(|index| index == "idx_records_session_seq"));
    let lane_move_indexes = index_names(&db, "lane_moves");
    assert!(
        !lane_move_indexes
            .iter()
            .any(|index| index == "idx_lane_moves_session_lane_seq")
    );
}
