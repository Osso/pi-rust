use super::*;
use std::{cell::RefCell, rc::Rc, sync::Mutex};

#[tokio::test]
async fn html_export_uses_the_original_custom_tool_call_and_result_renderers() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    let mut definition = custom_grep_definition(Arc::new(Mutex::new(Vec::new())));
    definition.approval_required = Some(false);
    definition.render_call = Some(Arc::new(|_, _, _| {
        Ok(Rc::new(RefCell::new(pi_tui::components::text::Text::new(
            "SESSION-CALL-RENDER",
            0,
            0,
        ))))
    }));
    definition.render_result = Some(Arc::new(|_, options, _, _| {
        Ok(Rc::new(RefCell::new(pi_tui::components::text::Text::new(
            format!("SESSION-RESULT-RENDER expanded={}", options.expanded),
            0,
            0,
        ))))
    }));
    let fixture = fixture_with_multi_agent(
        Default::default(),
        Vec::new(),
        Some(vec!["grep".to_owned(), "end_turn".to_owned()]),
        None,
        FixtureOverrides {
            persist_session: true,
            custom_tool_definitions: vec![definition],
            ..Default::default()
        },
    )
    .await;
    fixture.faux.set_responses(vec![
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "grep",
                serde_json::from_value(json!({"pattern":"needle"})).unwrap(),
                Some("grep-call".to_owned()),
            )],
            Default::default(),
        )
        .into(),
        faux_assistant_message(
            vec![pi_ai::providers::faux::faux_tool_call(
                "end_turn",
                serde_json::from_value(json!({"reason":"done"})).unwrap(),
                Some("end".to_owned()),
            )],
            Default::default(),
        )
        .into(),
    ]);
    fixture.session.prompt("find needle", Default::default()).await.unwrap();
    let output = fixture._temp_dir.path().join("custom.html");
    fixture
        .session
        .export_to_html(Some(&output.display().to_string().into()))
        .await
        .unwrap();
    let html = std::fs::read_to_string(output).unwrap();
    let start = html
        .find("<script id=\"session-data\" type=\"application/json\">")
        .unwrap()
        + "<script id=\"session-data\" type=\"application/json\">".len();
    let encoded = &html[start..start + html[start..].find("</script>").unwrap()];
    let bytes = pi_js_compat::base64::atob(&encoded.into()).unwrap();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    fixture.session.dispose();
    let rendered = &data["renderedTools"]["grep-call"];
    assert!(
        rendered["callHtml"]
            .as_str()
            .unwrap_or_default()
            .contains("SESSION-CALL-RENDER"),
        "{rendered}"
    );
    assert!(
        rendered["resultHtmlCollapsed"]
            .as_str()
            .unwrap_or_default()
            .contains("SESSION-RESULT-RENDER expanded=false"),
        "{rendered}"
    );
    assert!(
        rendered["resultHtmlExpanded"]
            .as_str()
            .unwrap_or_default()
            .contains("SESSION-RESULT-RENDER expanded=true"),
        "{rendered}"
    );
}
