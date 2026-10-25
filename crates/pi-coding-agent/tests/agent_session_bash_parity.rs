use super::*;
use pi_coding_agent::core::agent_session::ExecuteBashOptions;
use pi_coding_agent::core::tools::bash::{BashExecOptions, BashExecResult, BashOperations};
use std::sync::Mutex;

#[derive(Default)]
struct CapturingBash {
    seen: Mutex<Vec<(JsString, pi_js_compat::abort::AbortSignal)>>,
    release: Option<Arc<tokio::sync::Notify>>,
}
impl BashOperations for CapturingBash {
    fn exec<'a>(
        &'a self,
        command: &'a JsString,
        _cwd: &'a JsString,
        options: BashExecOptions,
    ) -> pi_ai::request::BoxFuture<'a, Result<BashExecResult, pi_agent_core::agent::ToolError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push((command.clone(), options.signal.expect("session signal")));
            if let Some(release) = &self.release {
                release.notified().await;
            }
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}

#[tokio::test]
async fn empty_shell_prefix_does_not_add_a_newline_to_the_executed_command() {
    let fixture = fixture_with(
        serde_json::from_value(json!({"shellCommandPrefix":""})).unwrap(),
        Vec::new(),
        None,
        Some(Vec::new()),
    )
    .await;
    let operations = Arc::new(CapturingBash::default());
    fixture
        .session
        .execute_bash(
            &"printf hi".into(),
            None,
            ExecuteBashOptions {
                operations: Some(operations.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(operations.seen.lock().unwrap()[0].0, "printf hi");
    fixture.session.dispose();
}

#[tokio::test]
async fn bash_commit_observers_see_execution_active_until_finally() {
    let fixture = fixture().await;
    let target = Arc::new(Mutex::new(Some(fixture.session.clone())));
    let callback = Arc::clone(&target);
    let states = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&states);
    fixture.session.subscribe(Arc::new(move |event| {
        if matches!(event, AgentSessionEvent::BashMessagesCommitted(_)) {
            captured
                .lock()
                .unwrap()
                .push(callback.lock().unwrap().as_ref().unwrap().is_bash_running());
        }
    }));
    fixture
        .session
        .execute_bash(
            &"true".into(),
            None,
            ExecuteBashOptions {
                operations: Some(Arc::new(CapturingBash::default())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    *target.lock().unwrap() = None;
    fixture.session.dispose();
    assert_eq!(*states.lock().unwrap(), vec![true]);
}

#[tokio::test]
async fn overlapping_bash_abort_targets_only_the_latest_and_first_completion_clears_tracking() {
    let fixture = fixture().await;
    let release = Arc::new(tokio::sync::Notify::new());
    let operations = Arc::new(CapturingBash {
        seen: Mutex::new(Vec::new()),
        release: Some(Arc::clone(&release)),
    });
    let mut tasks = Vec::new();
    for index in 0..2 {
        let session = fixture.session.clone();
        let ops = operations.clone();
        tasks.push(tokio::spawn(async move {
            session
                .execute_bash(
                    &JsString::from(format!("command {index}")),
                    None,
                    ExecuteBashOptions {
                        operations: Some(ops),
                        ..Default::default()
                    },
                )
                .await
        }));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while operations.seen.lock().unwrap().len() <= index {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    fixture.session.abort_bash();
    let states = operations
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|(_, signal)| signal.aborted())
        .collect::<Vec<_>>();
    release.notify_one();
    tasks.remove(0).await.unwrap().unwrap();
    let tracking_after_first = fixture.session.is_bash_running();
    release.notify_one();
    tasks.remove(0).await.unwrap().unwrap();
    fixture.session.dispose();
    assert_eq!(states, vec![false, true]);
    assert!(
        !tracking_after_first,
        "frozen finally clears the single tracked controller"
    );
}
