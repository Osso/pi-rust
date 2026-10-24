// Frozen interactive-mode.ts:3939-3948, 5083-5107: live metadata survives summary edits.
#[tokio::test(flavor = "current_thread")]
async fn live_compaction_display_and_edits_match_frozen_frames() {
    tokio::task::LocalSet::new().run_until(async {
        let mut source = crate::test_oracle::TsOracle::spawn(
            "interactive-live-compaction-oracle.mts", "packages/coding-agent/src",
        );
        for (persisted, expanded, width) in [(true, false, 100), (true, true, 35), (false, true, 100)] {
            let (mode, _terminal, _faux, _temp) = build_mode().await;
            let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter()
                .map(|name| ((*name).into(), crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
            crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(
                &colors, &colors, crate::modes::interactive::theme::ColorMode::Color256, None, None,
            ).unwrap()));
            mode.tool_output_expanded.set(expanded);
            let entry_id = {
                let session = mode.session();
                let mut manager = session.session_manager();
                let first = manager.append_message(AgentMessage::User(pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::Tag, content: pi_ai::types::UserContent::Text("kept user".into()),
                    input_source: None, provider_native: None, timestamp: 0.0,
                })).unwrap();
                persisted.then(|| manager.append_compaction(&"persisted summary".into(), &first, 999.0,
                    crate::core::session_manager::CompactionAppendOptions { duration_ms: Some(31.0), ..Default::default() }).unwrap())
            };
            let mut result = compaction_result(None);
            result.summary = JsString::from_utf16(vec![0x6c, 0x69, 0x76, 0x65, 0xd800]);
            result.duration_ms = Some(1250.0);
            result.estimated_tokens_after = Some(53.0);
            result.kept_from_previous_context_tokens = Some(21.0);
            result.compacted_result_tokens = Some(32.0);
            let edits: Vec<JsString> = if persisted { vec![JsString::from_utf16(vec![0x65, 0x64, 0x69, 0x74, 0xdc00]), "second edit".into()] } else { Vec::new() };
            let request = JsValue::Object(JsObject::from([
                ("cwd".into(), JsValue::String(mode.cwd())),
                ("colors".into(), js_json::to_value(&crate::modes::interactive::theme::REQUIRED_COLORS).unwrap()),
                ("width".into(), JsValue::Number(width as f64)),
                ("expanded".into(), JsValue::Bool(expanded)),
                ("persisted".into(), JsValue::Bool(persisted)),
                ("result".into(), js_json::to_value(&result).unwrap()),
                ("edits".into(), js_json::to_value(&edits).unwrap()),
            ]));
            let expected = source.request(&request);
            assert_eq!(expected.get("ok"), Some(&JsValue::Bool(true)), "{expected:?}");
            mode.chat_container.borrow_mut().clear();
            mode.rebuild_chat_after_compaction(&result);
            let frame = |mode: &InteractiveMode| js_json::to_value(&mode.chat_container.borrow_mut().render(width)
                .iter().map(|line| line.units().to_vec()).collect::<Vec<_>>()).unwrap();
            let mut frames = vec![frame(&mode)];
            for summary in edits {
                let entry = mode.session().session_manager().get_entry(entry_id.as_ref().unwrap()).unwrap().as_compaction().unwrap().into_owned();
                mode.start_compaction_summary_edit(&entry, entry.summary.clone()).unwrap();
                assert!(mode.submit_pending_compaction_summary_edit(&summary).unwrap());
                frames.push(frame(&mode));
            }
            assert_eq!(JsValue::Array(frames), expected.get("frames").unwrap().clone(), "persisted={persisted},expanded={expanded},width={width}");
            assert_eq!(mode.editor().borrow().get_text().unwrap().units(), expected.get("editor").unwrap().as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u16).collect::<Vec<_>>());
        }
    }).await;
}
