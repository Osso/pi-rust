#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::footer_data_provider::{BranchChangeSubscription, FooterDataProvider};
use pi_js_compat::js_json;
use std::sync::{Arc, Mutex};

const SOURCE: &str = "packages/coding-agent/src/core/footer-data-provider.ts";

#[tokio::test]
async fn branch_callbacks_follow_live_set_removal_addition_and_disposal() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("footer-provider-oracle.mts", SOURCE);
    for operation in ["live", "dispose"] {
        let expected = source.request(&js_json!({"operation":operation,"cwd":cwd}));
        let provider = Arc::new(FooterDataProvider::new(cwd));
        let trace = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriptions = Arc::new(Mutex::new(Vec::<BranchChangeSubscription>::new()));
        let first = Arc::new(Mutex::new(None));
        let second = Arc::new(Mutex::new(None));
        let first_callback = first.clone();
        let second_callback = second.clone();
        let callback_provider = Arc::downgrade(&provider);
        let callback_trace = trace.clone();
        let retained = subscriptions.clone();
        *first.lock().unwrap() = Some(provider.on_branch_change(move || {
            let provider = callback_provider.upgrade().unwrap();
            if operation == "dispose" {
                callback_trace.lock().unwrap().push("dispose".into());
                provider.dispose();
            } else {
                callback_trace.lock().unwrap().push("first".into());
                drop(first_callback.lock().unwrap().take());
                drop(second_callback.lock().unwrap().take());
                let added_trace = callback_trace.clone();
                retained.lock().unwrap().push(provider.on_branch_change(move || {
                    added_trace.lock().unwrap().push("added".into());
                }));
            }
        }));
        let second_trace = trace.clone();
        *second.lock().unwrap() = Some(provider.on_branch_change(move || {
            second_trace.lock().unwrap().push("removed".into());
        }));
        provider.set_cwd(format!("{cwd}/next"));
        if operation == "live" {
            provider.set_cwd(format!("{cwd}/again"));
        }
        let actual = js_json!(*trace.lock().unwrap());
        provider.dispose();
        assert_eq!(actual, expected, "{operation}");
    }
}
