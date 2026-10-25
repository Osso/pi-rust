#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_agent_core::AgentToolResult;
use pi_coding_agent::bundled_extensions::goal::goal_scheduling::{
    GoalSchedulingOptions, GoalWaitCallbacks, create_goal_scheduler,
};
use pi_coding_agent::core::extensions::{ExtensionApi, ExtensionRunner};
use pi_js_compat::js_json as json;
use std::sync::{Arc, Mutex};

#[tokio::test(start_paused = true)]
async fn goal_scheduling_review_timeout_wins_over_a_live_agent_wait() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let runtime = support::runtime();
    let api = Arc::new(Mutex::new(None::<ExtensionApi>));
    let capture = api.clone();
    let extension = support::extension(&runtime, cwd, "scheduler", move |api| {
        *capture.lock().unwrap() = Some(api.clone());
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        support::session_manager(cwd),
        support::model_registry().await,
        None,
    );
    let (release, wait_result) = tokio::sync::oneshot::channel::<()>();
    let wait_result = Arc::new(Mutex::new(Some(wait_result)));
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let tools = events.clone();
    let mut actions = support::actions();
    actions.call_tool = Arc::new(move |name, _, _, _| {
        tools.lock().unwrap().push(name.clone());
        let wait = if name == "wait_agent" {
            wait_result.lock().unwrap().take()
        } else {
            None
        };
        Box::pin(async move {
            if let Some(wait) = wait {
                wait.await.unwrap();
            }
            Ok(AgentToolResult {
                content: Vec::new(),
                details: Some(json!({"activeCount":1})),
                is_error: None,
                terminate: None,
            })
        })
    });
    runner.bind_core(actions, support::context_actions(), None).unwrap();
    let review_events = events.clone();
    let apply_events = events.clone();
    let errors = events.clone();
    let scheduler = create_goal_scheduler(GoalSchedulingOptions {
        pi: api.lock().unwrap().take().unwrap(),
        review_goal: Arc::new(move |_, _, _, _| {
            review_events.lock().unwrap().push("review".into());
            Box::pin(async { Ok(()) })
        }),
        apply_decision: Arc::new(move |_, _, _, _| {
            apply_events.lock().unwrap().push("apply".into());
            Box::pin(async { Ok(()) })
        }),
        is_same_running_goal: Arc::new(|_, _: ()| Ok(true)),
        report_error: Arc::new(move |error, _| errors.lock().unwrap().push(format!("error:{:?}", error.message))),
    });
    let waiting = events.clone();
    let scheduled = events.clone();
    let waking = events.clone();
    scheduler
        .wait_for_agents_or_schedule_review(
            runner.create_context(),
            (),
            Vec::new(),
            GoalWaitCallbacks {
                on_agent_wait: Arc::new(move |review_at| {
                    assert!(pi_js_compat::date::date_parse_millis(review_at).is_some());
                    waiting.lock().unwrap().push("waiting".into());
                    Ok(())
                }),
                on_agent_wake: Arc::new(move || {
                    waking.lock().unwrap().push("wake".into());
                    Ok(())
                }),
                on_review_scheduled: Arc::new(move |_| {
                    scheduled.lock().unwrap().push("scheduled".into());
                    Ok(())
                }),
            },
        )
        .await
        .unwrap();
    tokio::task::yield_now().await;
    assert_eq!(*events.lock().unwrap(), ["list_agents", "waiting", "wait_agent"]);
    tokio::time::advance(std::time::Duration::from_secs(300)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        *events.lock().unwrap(),
        ["list_agents", "waiting", "wait_agent"],
        "frozen review delay is 25 minutes, not the old five"
    );
    tokio::time::advance(std::time::Duration::from_secs(1200)).await;
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert_eq!(
        *events.lock().unwrap(),
        ["list_agents", "waiting", "wait_agent", "review", "apply"],
        "review deadline runs while agents are still waiting"
    );
    release.send(()).unwrap();
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.as_str() == "review")
            .count(),
        1,
        "timeout and wake claim the same epoch"
    );
    let mut ts = oracle::TsOracle::spawn(
        "goal-scheduling-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-scheduling.ts",
    );
    let expected = ts.request(&json!({"id":1}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(json!(events.lock().unwrap().clone()), expected["output"]);
    let unhandled = ts.request(&json!({"id":2, "operation":"report-error"}));
    assert_eq!(unhandled["ok"], json!(true));
    assert_eq!(
        unhandled["output"],
        json!("status append failed"),
        "reportError thrown inside the detached wait catch is process-uncaught in frozen"
    );
    scheduler.clear_all();
}
