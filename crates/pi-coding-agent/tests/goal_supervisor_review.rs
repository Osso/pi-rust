#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::goal::goal_types::{GoalReviewInput, GoalReviewKind, GoalSupervisorResponse};
use pi_coding_agent::bundled_extensions::goal::supervisor_review::with_supervisor_review_status;
use pi_coding_agent::core::extensions::types::HandlerError;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject, JsValue as Value};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn goal_resident_supervisor_review_cancels_and_unregisters_like_frozen() {
    use pi_coding_agent::bundled_extensions::goal::supervisor_review::review_goal_with_resident_supervisor;
    let mut ts = oracle::TsOracle::spawn(
        "goal-supervisor-review-oracle.ts",
        "packages/coding-agent/extensions/goal/src/supervisor-review.ts",
    );
    let expected =
        ts.request(&json!({"id":1, "operation":"resident", "cases":[{"standalone":false}, {"standalone":true}]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let mut actual = Vec::new();
    for standalone in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut harness = bundled::Harness::new(
            Vec::new(),
            pi_coding_agent::core::extensions::create_extension_runtime(),
            bundled::HarnessOptions::new(directory.path().to_str().unwrap()),
        )
        .await;
        let events = Arc::new(Mutex::new(Vec::new()));
        let record = events.clone();
        let mut actions = support::context_actions();
        actions.get_no_supervisor = Some(Arc::new(move || standalone));
        actions.register_supervisor_review_cancellation = Some(Arc::new(move |handler| {
            record.lock().unwrap().push("register");
            handler();
            let record = record.clone();
            Ok(Arc::new(move || record.lock().unwrap().push("unregister")) as Arc<dyn Fn() + Send + Sync>)
        }));
        harness.runner.bind_core(support::actions(), actions, None).unwrap();
        let reviewed = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            review_goal_with_resident_supervisor(GoalReviewInput {
                ctx: harness.context(),
                kind: GoalReviewKind::GoalIdleReview,
                payload: JsObject::new(),
            }),
        )
        .await
        .expect("cancelled review must not poll the resident supervisor");
        actual.push(match reviewed {
            Ok(result) => json!({"result":result,"events":events.lock().unwrap().clone()}),
            Err(error) => json!({"error":error.message,"events":events.lock().unwrap().clone()}),
        });
    }
    assert_eq!(json!(actual), expected["output"]);
}

#[tokio::test]
async fn goal_supervisor_review_groups_waiting_and_final_status_like_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let harness = bundled::Harness::new(
        Vec::new(),
        pi_coding_agent::core::extensions::create_extension_runtime(),
        bundled::HarnessOptions::new(directory.path().to_str().unwrap()),
    )
    .await;
    let cases = json!([
        {"result":{"kind":"continue", "instructions":"continue work", "reason":"not done"}},
        {"result":{"kind":"complete", "reason":"done"}},
        {"result":{"kind":"set", "objective":"new objective", "reason":"set"}},
        {"result":{"kind":"pause", "reason":"blocked"}},
        {"result":{"kind":"wait", "reason":"agents running"}},
        {"result":{"kind":"error", "reason":"failed"}},
        {"error":"  keep error whitespace  "}, {"error":""}
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "goal-supervisor-review-oracle.ts",
        "packages/coding-agent/extensions/goal/src/supervisor-review.ts",
    );
    let expected = ts.request(&json!({"id":1, "cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let mut snapshots = Vec::new();
    for case in cases.as_array().unwrap() {
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let capture = statuses.clone();
        let case = case.clone();
        let display_id = Arc::new(Mutex::new(None));
        let id_capture = display_id.clone();
        let review = with_supervisor_review_status(
            Arc::new(move |_ctx, message, review_at, id| {
                let mut first_id = id_capture.lock().unwrap();
                if first_id.is_none() {
                    *first_id = id.cloned();
                }
                assert_eq!(first_id.as_ref(), id);
                let mut status = JsObject::new();
                status.insert("message".into(), Value::String(message.clone()));
                status.insert("displayId".into(), Value::String("display-one".into()));
                if let Some(review_at) = review_at {
                    status.insert("reviewAt".into(), Value::String(review_at.into()));
                }
                capture.lock().unwrap().push(Value::Object(status));
                Ok(())
            }),
            Arc::new(move |_input| {
                let case = case.clone();
                Box::pin(async move {
                    if let Some(error) = case["error"].as_string() {
                        Err(HandlerError::new(error.clone()))
                    } else {
                        Ok(json::from_value::<GoalSupervisorResponse>(&case["result"]).unwrap())
                    }
                })
            }),
        );
        let result = review(GoalReviewInput {
            ctx: harness.context(),
            kind: GoalReviewKind::GoalIdleReview,
            payload: JsObject::new(),
        })
        .await
        .unwrap();
        assert_eq!(result.display_id(), display_id.lock().unwrap().as_ref());
        let mut result = json::to_value(&result).unwrap();
        result
            .as_object_mut()
            .unwrap()
            .insert("displayId".into(), Value::String("display-one".into()));
        snapshots.push(json!({"statuses":statuses.lock().unwrap().clone(), "result":result}));
    }
    assert_eq!(json!(snapshots), expected["output"]);
}
