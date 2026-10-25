use super::*;
use pi_js_compat::{js_json, js_json::JsValue};

struct PausedGit {
    branch: Mutex<JsString>,
    executions: AtomicUsize,
    hold: AtomicBool,
    release: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}
impl GitBranchRunner for PausedGit {
    fn resolve_sync(&self, _: &JsString) -> Option<JsString> {
        Some(lock(&self.branch).clone())
    }
    fn resolve_async(&self, _: &JsString) -> BoxFuture<'static, Option<JsString>> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        let value = lock(&self.branch).clone();
        let pause = if self.hold.swap(false, Ordering::SeqCst) {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            *lock(&self.release) = Some(sender);
            Some(receiver)
        } else {
            None
        };
        Box::pin(async move {
            if let Some(pause) = pause {
                pause.await.expect("private git release");
            }
            Some(value)
        })
    }
}
async fn native_refresh_scenario(operation: &str) -> JsValue {
    let root = tempfile::tempdir().unwrap();
    let table = root.path().join(".git/reftable");
    let list = table.join("tables.list");
    let head = root.path().join(".git/HEAD");
    std::fs::create_dir_all(&table).unwrap();
    std::fs::write(&head, "ref: refs/heads/.invalid\n").unwrap();
    std::fs::write(&list, "0\n").unwrap();
    let pending = operation.ends_with("pending") || operation == "callback_recovery";
    let git = Arc::new(PausedGit {
        branch: Mutex::new("main".into()),
        executions: AtomicUsize::new(0),
        hold: AtomicBool::new(pending),
        release: Mutex::new(None),
    });
    let provider = FooterDataProvider::with_git_runner(root.path().to_str().unwrap(), git.clone());
    let changes = Arc::new(Mutex::new(Vec::new()));
    let callback_changes = changes.clone();
    let callback_provider = provider.clone();
    let throws = Arc::new(AtomicBool::new(operation == "callback_recovery"));
    let _subscription = provider.on_branch_change(move || {
        lock(&callback_changes).push(callback_provider.get_git_branch());
        if throws.swap(false, Ordering::SeqCst) {
            return Err(pi_js_compat::node_error::NativeNodeError::from_constructor(
                pi_js_compat::node_error::NativeErrorClass::TypeError,
                JsString::from("refresh-") + JsString::from_utf16(vec![0xd800]),
            )
            .with_property("code", JsValue::String("EREFRESH".into()))
            .into());
        }
        Ok(())
    });
    let initial = if operation == "unread" {
        Some("unread".into())
    } else {
        provider.get_git_branch()
    };
    *lock(&git.branch) = if operation == "same" {
        "main".into()
    } else {
        "next".into()
    };
    let direct = if operation == "callback_recovery" {
        let provider = provider.clone();
        Some(tokio::spawn(async move { provider.refresh_git_branch_async().await }))
    } else {
        None
    };
    if operation == "read_failure" {
        std::fs::remove_file(&head).unwrap();
    } else if direct.is_none() {
        std::fs::write(&list, "1\n").unwrap();
    }
    if pending {
        tokio::time::timeout(Duration::from_secs(5), async {
            while lock(&git.release).is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        std::fs::write(&list, "2\n").unwrap();
        std::fs::write(&list, "3\n").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        if operation == "dispose_pending" {
            provider.dispose();
        }
        if operation == "cwd_pending" {
            let next = root.path().join("other");
            std::fs::create_dir_all(next.join(".git")).unwrap();
            std::fs::write(next.join(".git/HEAD"), "ref: refs/heads/direct\n").unwrap();
            provider.set_cwd(next.to_str().unwrap()).unwrap();
        }
        *lock(&git.branch) = "last".into();
        lock(&git.release).take().unwrap().send(()).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(1700)).await;
    let final_value = js_json!({"branch":provider.get_git_branch(),"changes":*lock(&changes),"executions":git.executions.load(Ordering::SeqCst)});
    provider.dispose();
    if let Some(direct) = direct {
        let error = direct.await.unwrap().unwrap_err();
        let value = match error.thrown_value() {
            pi_js_compat::abort::AbortReason::Error(error) => error.value(),
            _ => panic!("original refresh Error"),
        };
        return js_json!({"initial":initial,"final":final_value,"error":value});
    }
    js_json!({"initial":initial,"final":final_value})
}
#[tokio::test]
async fn pending_refreshes_coalesce_and_retain_frozen_disposal_cwd_and_lazy_cache_boundaries() {
    let mut source = crate::test_oracle::TsOracle::spawn(
        "footer-refresh-oracle.mts",
        "packages/coding-agent/src/core/footer-data-provider.ts",
    );
    for operation in [
        "pending",
        "dispose_pending",
        "cwd_pending",
        "unread",
        "same",
        "read_failure",
        "callback_recovery",
    ] {
        let expected = source.request(&js_json!({"operation":operation}));
        let actual = native_refresh_scenario(operation).await;
        assert_eq!(actual, expected, "{operation}");
    }
}
