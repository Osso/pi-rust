use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{with_control_db, with_immediate_transaction};
use pi_coding_agent::core::sqlite::ControlDbError;
use pi_js_compat::{
    abort::AbortReason,
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::{Value, json};

#[test]
fn connection_and_transaction_callbacks_preserve_original_throw_and_commit_boundary() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "storage-control-callback-oracle.mts",
        "packages/coding-agent/src/core/session-control-db.ts",
    );
    let mut case = 0;
    for transaction in [false, true] {
        for (failure, typed) in [
            (Value::Null, false),
            (json!("failed 漢😀"), false),
            (json!({"detail":"failed"}), false),
            (Value::Null, true),
        ] {
            let expected=source.request(&json!({"id":1,"db":root.path().join(format!("source{case}.sqlite")),"transaction":transaction,"failure":failure,"typed":typed}));
            let db = JsString::from(root.path().join(format!("native{case}.sqlite")).to_str().unwrap());
            case += 1;
            with_control_db(&db, |connection| {
                connection.execute_batch("CREATE TABLE callback_probe(value INTEGER)")?;
                Ok(())
            })
            .unwrap();
            let callback = if typed {
                CallbackError::from(
                    NativeNodeError::from_constructor(NativeErrorClass::TypeError, "callback failed")
                        .with_property("code", JsValue::String("FIXTURE".into())),
                )
            } else {
                CallbackError::from_rejection(AbortReason::Value(JsValue::from(failure)))
            };
            let outcome: Result<(), ControlDbError> = with_control_db(&db, |connection| {
                let operation = |connection: &rusqlite::Connection| -> Result<(), ControlDbError> {
                    connection.execute("INSERT INTO callback_probe VALUES (1)", [])?;
                    Err(ControlDbError::from(callback.clone()))
                };
                if transaction {
                    with_immediate_transaction(connection, operation)
                } else {
                    operation(connection)
                }
            });
            let thrown = CallbackError::from(outcome.unwrap_err()).thrown_value();
            let error = match thrown {
                AbortReason::Value(value) => value,
                AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
            };
            let count = with_control_db(&db, |connection| {
                Ok(connection.query_row("SELECT COUNT(*) FROM callback_probe", [], |row| row.get::<_, i64>(0))?)
            })
            .unwrap();
            assert_eq!(
                JsValue::from(json!({"id":1,"error":error,"count":{"count":count}})),
                JsValue::from(expected)
            );
        }
    }
}
