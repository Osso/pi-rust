use crate::shared_extensions as support;
use crate::shared_oracle as oracle;

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

#[tokio::test]
async fn goal_scheduling_wait_wake_error_and_clear_match_frozen() {
    let mut ts = oracle::TsOracle::spawn(
        "goal-scheduling-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-scheduling.ts",
    );
    for variant in ["none", "list-error", "wait-error", "wake", "changed", "clear"] {
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
        let events = Arc::new(Mutex::new(Vec::new()));
        let (release, wait) = tokio::sync::oneshot::channel::<()>();
        let wait = Arc::new(Mutex::new(Some(wait)));
        let tools = events.clone();
        let mut actions = support::actions();
        actions.call_tool = Arc::new(move |name, _, _, _| {
            tools.lock().unwrap().push(json!(name));
            let waiting = if name == "wait_agent" {
                wait.lock().unwrap().take()
            } else {
                None
            };
            Box::pin(async move {
                if let Some(waiting) = waiting {
                    waiting.await.unwrap();
                }
                Ok(AgentToolResult {
                    content: Vec::new(),
                    details: Some(if name == "list_agents" {
                        json!({"activeCount": if variant == "none" {0} else {1}})
                    } else {
                        json!(null)
                    }),
                    is_error: Some(
                        variant == "list-error" && name == "list_agents"
                            || variant == "wait-error" && name == "wait_agent",
                    ),
                    terminate: None,
                })
            })
        });
        runner.bind_core(actions, support::context_actions(), None).unwrap();
        let review = events.clone();
        let apply = events.clone();
        let errors = events.clone();
        let scheduler = create_goal_scheduler(GoalSchedulingOptions {
            pi: api.lock().unwrap().take().unwrap(),
            review_goal: Arc::new(move |_, _, _, evidence| {
                review.lock().unwrap().push(json!(["review", evidence]));
                Box::pin(async { Ok(()) })
            }),
            apply_decision: Arc::new(move |_, _, _, _| {
                apply.lock().unwrap().push(json!("apply"));
                Box::pin(async { Ok(()) })
            }),
            is_same_running_goal: Arc::new(move |_, _: ()| Ok(variant != "changed")),
            report_error: Arc::new(move |error, _| {
                errors
                    .lock()
                    .unwrap()
                    .push(json!(pi_js_compat::js_json::JsString::from("error:") + error.message))
            }),
        });
        let waiting = events.clone();
        let wake = events.clone();
        let scheduled = events.clone();
        scheduler
            .wait_for_agents_or_schedule_review(
                runner.create_context(),
                (),
                Vec::new(),
                GoalWaitCallbacks {
                    on_agent_wait: Arc::new(move |_| {
                        waiting.lock().unwrap().push(json!("waiting"));
                        Ok(())
                    }),
                    on_agent_wake: Arc::new(move || {
                        wake.lock().unwrap().push(json!("wake"));
                        Ok(())
                    }),
                    on_review_scheduled: Arc::new(move |_| {
                        scheduled.lock().unwrap().push(json!("scheduled"));
                        Ok(())
                    }),
                },
            )
            .await
            .unwrap();
        if variant == "clear" {
            scheduler.clear_all();
        }
        let _ = release.send(());
        for _ in 0..12 {
            tokio::task::yield_now().await;
        }
        let expected = ts.request(&json!({"id":1,"operation":"branches","variant":variant}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(json!(events.lock().unwrap().clone()), expected["output"], "{variant}");
        scheduler.clear_all();
    }
}

#[tokio::test(start_paused = true)]
async fn goal_completion_scheduling_decision_matrix_matches_frozen() {
    use pi_coding_agent::bundled_extensions::goal::{
        completion_scheduling::{CompletionSchedulingOptions, create_completion_wait_scheduler},
        goal_types::{GoalSupervisorResponse, ReviewedGoalResponse},
    };
    let decisions = json!([
        {"kind":"complete","reason":"ok","displayId":"group"},
        {"kind":"continue","reason":"more","instructions":"check logs","displayId":"group"},
        {"kind":"wait","reason":"workers","displayId":"group"},
        {"kind":"pause","reason":"blocked","displayId":"group"},
        {"kind":"error","reason":"failed","displayId":"group"},
        {"kind":"set","reason":"unexpected","objective":"new goal","displayId":"group"}
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "goal-completion-oracle.ts",
        "packages/coding-agent/extensions/goal/src/completion-scheduling.ts",
    );
    for decision in decisions.as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().to_str().unwrap();
        let runtime = support::runtime();
        let api = Arc::new(Mutex::new(None::<ExtensionApi>));
        let capture = api.clone();
        let extension = support::extension(&runtime, cwd, "completion", move |api| {
            *capture.lock().unwrap() = Some(api.clone())
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
        let events = Arc::new(Mutex::new(Vec::new()));
        let tools = events.clone();
        let mut actions = support::actions();
        actions.call_tool = Arc::new(move |_, _, _, _| {
            tools.lock().unwrap().push(json!("list"));
            Box::pin(async {
                Ok(AgentToolResult {
                    content: Vec::new(),
                    details: Some(json!({"activeCount":0})),
                    is_error: None,
                    terminate: None,
                })
            })
        });
        runner.bind_core(actions, support::context_actions(), None).unwrap();
        let review = events.clone();
        let consume = events.clone();
        let complete = events.clone();
        let continued = events.clone();
        let status = events.clone();
        let clear = events.clone();
        let error = events.clone();
        let response: GoalSupervisorResponse = pi_js_compat::js_json::from_value(decision).unwrap();
        let scheduler = create_completion_wait_scheduler(CompletionSchedulingOptions {
            pi: api.lock().unwrap().take().unwrap(),
            review_goal: Arc::new(move |input| {
                review
                    .lock()
                    .unwrap()
                    .push(json!(["review", pi_js_compat::js_json::JsValue::Object(input.payload)]));
                let decision = response.clone();
                Box::pin(async move {
                    Ok(ReviewedGoalResponse {
                        decision,
                        evidence_count: 3,
                    })
                })
            }),
            consume_review_evidence: Arc::new(move |_, _, count| {
                consume.lock().unwrap().push(json!(["consume", count]));
                Ok(())
            }),
            is_same_goal: Arc::new(|_, _| Ok(true)),
            on_complete: Arc::new(move |_, _| {
                complete.lock().unwrap().push(json!("complete"));
                Ok(())
            }),
            on_continue: Arc::new(move |instructions, id| {
                continued.lock().unwrap().push(json!(["continue", instructions, id]));
                Ok(())
            }),
            on_status: Arc::new(move |_, message, at, id| {
                status
                    .lock()
                    .unwrap()
                    .push(json!(["status", message, at.is_some(), id]));
                Ok(())
            }),
            on_clear_status: Arc::new(move |_| clear.lock().unwrap().push(json!("clear"))),
            on_error: Arc::new(move |err, _| error.lock().unwrap().push(json!(["error", err.message]))),
        });
        let ctx = runner.create_context();
        let guard = scheduler.create_review_guard(&ctx).unwrap();
        events.lock().unwrap().push(json!(["guard", guard().unwrap()]));
        let goal =
            pi_js_compat::js_json::from_value(&json!({"objective":"Ship it","branch":"main","createdAt":"2026-01-01"}))
                .unwrap();
        scheduler
            .wait(
                goal,
                ctx,
                &"tests pass".into(),
                &"first wait".into(),
                Some(&"initial".into()),
            )
            .await
            .unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(1500)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        events.lock().unwrap().push(json!(["guard", guard().unwrap()]));
        let expected = ts.request(&json!({"id":1,"decision":decision}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(
            json!(events.lock().unwrap().clone()),
            expected["output"],
            "{decision:?}"
        );
        scheduler.clear_all();
    }
}

#[tokio::test(start_paused = true)]
async fn goal_agent_end_cancellation_and_deferred_paths_match_frozen() {
    use pi_coding_agent::bundled_extensions::goal::{
        empty_response_scheduling::{EmptyResponseSchedulingOptions, create_empty_response_scheduler},
        goal_agent_end_scheduling::{ApplyIdleDecision, ScheduledGoalAgentEndOptions, run_scheduled_goal_agent_end},
        goal_types::{Goal, GoalSupervisorResponse, ReviewedGoalResponse},
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut ts = oracle::TsOracle::spawn(
        "goal-agent-end-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-agent-end-scheduling.ts",
    );
    for variant in ["none", "normal", "changed", "cancel", "defer-review", "defer-decision"] {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().to_str().unwrap();
        let runtime = support::runtime();
        let api = Arc::new(Mutex::new(None::<ExtensionApi>));
        let capture = api.clone();
        let extension = support::extension(&runtime, cwd, "agent-end", move |api| {
            *capture.lock().unwrap() = Some(api.clone())
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
        let pending = Arc::new(AtomicBool::new(variant == "defer-review"));
        let idle = Arc::new(AtomicBool::new(false));
        let mut context = support::context_actions();
        let pending_read = pending.clone();
        let idle_read = idle.clone();
        context.has_pending_messages = Arc::new(move || pending_read.load(Ordering::SeqCst));
        context.is_idle = Arc::new(move || idle_read.load(Ordering::SeqCst));
        runner.bind_core(support::actions(), context, None).unwrap();
        let api = api.lock().unwrap().take().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let applied = events.clone();
        let apply: ApplyIdleDecision = Arc::new(move |_, _, _, _| {
            applied.lock().unwrap().push("apply");
            Box::pin(async { Ok(()) })
        });
        let review = events.clone();
        let errors = events.clone();
        let scheduler = create_goal_scheduler(GoalSchedulingOptions {
            pi: api.clone(),
            review_goal: Arc::new(move |_, _: Goal, _, _| {
                review.lock().unwrap().push("review");
                Box::pin(async {
                    Ok(ReviewedGoalResponse {
                        decision: GoalSupervisorResponse::Complete {
                            display_id: None,
                            reason: "ok".into(),
                        },
                        evidence_count: 0,
                    })
                })
            }),
            apply_decision: apply.clone(),
            is_same_running_goal: Arc::new(|_, _| Ok(true)),
            report_error: Arc::new(move |_, _| errors.lock().unwrap().push("error")),
        });
        let ctx = runner.create_context();
        let cancel = scheduler.clone();
        let review = events.clone();
        let review_pending = pending.clone();
        let cancel_ctx = ctx.clone();
        let goal: Goal =
            pi_js_compat::js_json::from_value(&json!({"objective":"Ship it","branch":"main","createdAt":"2026-01-01"}))
                .unwrap();
        run_scheduled_goal_agent_end(ScheduledGoalAgentEndOptions {
            event: pi_coding_agent::core::extensions::types::AgentEndEvent {
                messages: Vec::new(),
                session_continuation: None,
            },
            ctx,
            scheduler: scheduler.clone(),
            empty_response_scheduler: create_empty_response_scheduler(EmptyResponseSchedulingOptions {
                pi: api,
                is_same_running_goal: Arc::new(|_, _: Goal| Ok(true)),
            }),
            apply_decision: apply,
            select_goal: Arc::new(move || Ok(if variant == "none" { None } else { Some(goal.clone()) })),
            is_same_goal: Arc::new(move |_, _| Ok(variant != "changed")),
            review_goal: Arc::new(move |_| {
                review.lock().unwrap().push("review");
                if variant == "cancel" {
                    cancel.clear_session(&cancel_ctx.session_manager().unwrap().get_session_id());
                }
                if variant == "defer-decision" {
                    review_pending.store(true, Ordering::SeqCst);
                }
                Box::pin(async {
                    Ok(ReviewedGoalResponse {
                        decision: GoalSupervisorResponse::Complete {
                            display_id: None,
                            reason: "ok".into(),
                        },
                        evidence_count: 0,
                    })
                })
            }),
        })
        .await
        .unwrap();
        for index in 0..2 {
            if index == 1 {
                idle.store(true, Ordering::SeqCst);
                pending.store(false, Ordering::SeqCst);
            }
            tokio::task::yield_now().await;
            tokio::time::advance(std::time::Duration::from_secs(1)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
        }
        let expected = ts.request(&json!({"id":1,"variant":variant}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(json!(events.lock().unwrap().clone()), expected["output"], "{variant}");
        scheduler.clear_all();
    }
}
