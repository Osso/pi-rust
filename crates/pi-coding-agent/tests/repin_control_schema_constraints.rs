//! Shared-schema constraints are exercised as writes, not SQL text assertions.
#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::read_session_metadata;
use serde_json::json;

#[test]
fn nested_legacy_artifacts_reject_insert_and_update_in_either_primary_initializer() {
    let temporary = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    for native_first in [false, true] {
        let path = temporary.path().join(format!("{native_first}.sqlite"));
        let path = path.to_str().unwrap();
        if native_first {
            read_session_metadata(path, "/missing").unwrap();
        } else {
            let result = source
                .request(&json!({"id":1,"steps":[{"op":"call","args":["readSessionMetadata",[path,"/missing"]]}]}));
            assert_eq!(result["output"][0]["ok"], true, "{result}");
        }
        let database = rusqlite::Connection::open(path).unwrap();
        for (table, key) in [
            ("multi_agent_agents", "agent_id"),
            ("multi_agent_mailbox_messages", "message_id"),
        ] {
            let valid = r#"{"nested":{"keep":"original"},"createdAt":"2099-01-01T00:00:00.000Z"}"#;
            database
                .execute(
                    &format!("INSERT INTO {table}(session_path,{key},data,updated_at) VALUES('/fixture/s','good',?,'initial')"),
                    [valid],
                )
                .unwrap();
            for field in ["artifactIds", "artifactRefs"] {
                let malformed = format!(r#"{{"nested":[{{"{field}":["old"]}}]}}"#);
                for sql in [
                    format!(
                        "INSERT INTO {table}(session_path,{key},data,updated_at) VALUES('/fixture/s','bad',?,'rejected')"
                    ),
                    format!("UPDATE {table} SET data=?,updated_at='rejected' WHERE {key}='good'"),
                ] {
                    let error = database.execute(&sql, [&malformed]).unwrap_err();
                    assert!(
                        error.to_string().contains("Legacy artifact fields are not supported"),
                        "{error}"
                    );
                }
                let (data, updated_at, rows): (String, String, i64) = database
                    .query_row(
                        &format!(
                            "SELECT data,updated_at,(SELECT COUNT(*) FROM {table}) FROM {table} WHERE {key}='good'"
                        ),
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .unwrap();
                assert_eq!((data.as_str(), updated_at.as_str(), rows), (valid, "initial", 1));
            }
        }
        drop(database);
        // The other initializer must preserve both original rows, not normalize them.
        if native_first {
            let result = source
                .request(&json!({"id":1,"steps":[{"op":"call","args":["readSessionMetadata",[path,"/missing"]]}]}));
            assert_eq!(result["output"][0]["ok"], true, "{result}");
        } else {
            read_session_metadata(path, "/missing").unwrap();
        }
        let database = rusqlite::Connection::open(path).unwrap();
        for table in ["multi_agent_agents", "multi_agent_mailbox_messages"] {
            let (rows, updated_at): (i64, String) = database
                .query_row(&format!("SELECT COUNT(*),MIN(updated_at) FROM {table}"), [], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .unwrap();
            assert_eq!((rows, updated_at.as_str()), (1, "initial"));
        }
    }
}
