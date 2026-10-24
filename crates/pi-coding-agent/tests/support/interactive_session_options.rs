#[tokio::test]
async fn extension_switch_and_fork_preserve_frozen_options_and_success_ui() {
    mod oracle {include!(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/support/oracle.rs"));}
    use crate::core::extensions::types::{SwitchSessionOptions,ForkOptions,WithSessionCallback};
    use pi_js_compat::js_json::{JsObject,JsValue};
    tokio::task::LocalSet::new().run_until(async {
        let seed=pi_js_compat::js_json::parse_lossless(r#""fork seed\ud800""#).unwrap().as_string().unwrap().clone();
        let mut source=oracle::TsOracle::spawn("interactive-session-options-oracle.mts","packages/coding-agent/src");
        for operation in ["fork-default","fork-at","switch"] {
            let (mode,_terminal,_faux,temp)=build_mode_with_persisted_session().await;
            mode.bind_extensions().await.unwrap();
            let session=mode.session();
            let entry_id={
                let mut manager=session.session_manager();
                let id=manager.append_message(AgentMessage::User(pi_ai::types::UserMessage{role:pi_ai::types::UserRole::Tag,content:pi_ai::types::UserContent::Text(seed.clone()),input_source:None,provider_native:None,timestamp:0.0})).unwrap();
                manager.append_message(AgentMessage::Assistant(Box::new(AssistantMessage::empty("faux","faux","faux-1",1.0)))).unwrap();
                id
            };
            mode.editor().borrow_mut().set_text(&"editor before".into()).unwrap();
            *mode.pending_compaction_summary_edit_id.borrow_mut()=Some("stale".into());
            let source_root=temp.path().join("source-options");
            std::fs::create_dir(&source_root).unwrap();
            let mut request=JsObject::from([
                ("root".into(),JsValue::String(source_root.to_str().unwrap().into())),
                ("operation".into(),JsValue::String(if operation=="switch" {"switch"} else {"fork"}.into())),
                ("seed".into(),JsValue::String(seed.clone())),
            ]);
            if operation=="fork-at" {request.insert("position".into(),JsValue::String("at".into()));}
            let expected=source.request(&JsValue::Object(request));
            assert_eq!(expected.get("called"),Some(&JsValue::Number(1.0)),"source: {expected:?}");
            let called=Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let callback:WithSessionCallback=Arc::new({let called=called.clone();move|context|{
                assert!(context.ui().is_ok(),"replacement UI must be bound before callback");
                called.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
                Box::pin(async{Ok(())})
            }});
            let context=session.extension_runner().create_command_context();
            let target=SessionManager::create(&mode.cwd(),Some(&js_path(&temp.path().join("switch-target"))),None).unwrap();
            let mut target=target;
            target.persist_for_recovery().unwrap();
            let target=target.get_session_file().unwrap().clone();
            let task=tokio::task::spawn_local(async move{
                if operation=="switch" {context.switch_session(target,Some(SwitchSessionOptions{with_session:Some(callback)})).await}
                else {context.fork(entry_id,Some(ForkOptions{position:if operation=="fork-at" {Some(crate::core::extensions::types::ForkPosition::At)} else {None},with_session:Some(callback)})).await}
            });
            tokio::time::timeout(Duration::from_secs(5),async{
                while !task.is_finished() {mode.handle_ui_requests().await;tokio::time::sleep(Duration::from_millis(1)).await;}
            }).await.expect("UI transition drain");
            let result=task.await.unwrap().unwrap();
            let mut actual=JsObject::from([
                ("called".into(),JsValue::Number(called.load(std::sync::atomic::Ordering::SeqCst) as f64)),
                ("cancelled".into(),JsValue::Bool(result.cancelled)),
                ("messageCount".into(),JsValue::Number(mode.session().session_manager().build_session_context().messages.len() as f64)),
            ]);
            if operation!="switch" {
                actual.insert("editor".into(),JsValue::String(mode.editor().borrow().get_text().unwrap()));
                actual.insert("summaryCleared".into(),JsValue::Bool(mode.pending_compaction_summary_edit_id.borrow().is_none()));
            }
            assert_eq!(JsValue::Object(actual),expected,"{operation}");
        }
    }).await;
}
