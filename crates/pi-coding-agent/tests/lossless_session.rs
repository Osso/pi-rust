use pi_agent_core::agent::AgentMessage;
use pi_coding_agent::core::session_manager::{FileEntry, SessionEntry, parse_session_entries};
use pi_coding_agent::core::session_tool_output::serialize_session_entry;
use pi_js_compat::js_json::parse_lossless;

#[test]
fn question_parser_and_answer_results_keep_raw_units_and_metadata() {
    use pi_coding_agent::core::tools::ask_questions::{format_answers, parse_params, question_result};
    let params = parse_lossless(r#"{"blocking":true,"questions":[{"question":"\ud800","options":[{"label":"\udfff"},{"label":"x"}]}],"metadata":{"\ud800":"\udfff"}}"#).unwrap();
    let parsed = parse_params(&params).unwrap();
    assert_eq!(parsed.questions[0].question.units(), &[0xd800]);
    let answers = parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap();
    let text = format_answers(&answers);
    let result = question_result(&params, text, answers.clone(), false);
    assert_eq!(result.details.as_ref().unwrap()["answers"], answers);
    assert_eq!(result.details.as_ref().unwrap()["metadata"], params["metadata"]);
}

#[test]
fn tool_truncation_keeps_units_until_the_source_utf8_suffix_boundary() {
    use pi_coding_agent::core::tools::truncate::{TruncationOptions, truncate_line, truncate_tail};
    use pi_js_compat::js_json::JsString;
    let text = JsString::from_utf16(vec![0xd800, 0x0a, 0xdfff]);
    let tail = truncate_tail(
        &text,
        TruncationOptions {
            max_lines: 1,
            max_bytes: 10,
        },
    );
    assert_eq!(tail.content.units(), &[0xdfff]);
    let pair = JsString::from_utf16(vec![0xd83d, 0xde00]);
    let line = truncate_line(&pair, Some(1));
    assert_eq!(line.text.units()[0], 0xd83d);
    let suffix = truncate_tail(
        &JsString::from_utf16(vec![0x78, 0xdfff]),
        TruncationOptions {
            max_lines: 2,
            max_bytes: 3,
        },
    );
    assert_eq!(suffix.content.units(), &[0xfffd]);
}

#[test]
fn permission_decisions_retain_updated_input_and_raw_bash_rule_text() {
    use pi_coding_agent::core::permissions::mcp_permission_prompt::{
        PermissionPromptDecision, parse_permission_prompt_decision,
    };
    use pi_coding_agent::core::rule_store::build_permission_rule_content;
    use pi_js_compat::js_json::JsString;
    let response = parse_lossless(r#"{"behavior":"allow","updatedInput":{"command":"\ud800"}}"#).unwrap();
    let Some(PermissionPromptDecision::Allow {
        updated_input: Some(input),
        ..
    }) = parse_permission_prompt_decision(&response)
    else {
        panic!("valid allow response");
    };
    assert_eq!(
        build_permission_rule_content("bash", &input),
        JsString::from_utf16(vec![0xd800])
    );
}

#[test]
fn prompt_template_arguments_retain_surrogates_and_quote_boundaries() {
    use pi_coding_agent::core::prompt_templates::{parse_command_args, substitute_args};
    use pi_js_compat::js_json::JsString;
    let input = JsString::from_utf16(vec![0x22, 0xd800, 0x20, 0xdfff, 0x22, 0x20, 0x78]);
    let args = parse_command_args(&input);
    assert_eq!(
        args,
        vec![JsString::from_utf16(vec![0xd800, 0x20, 0xdfff]), JsString::from("x")]
    );
    let expanded = substitute_args("[$1][$@]", &args);
    assert_eq!(
        expanded.units(),
        &[
            0x5b, 0xd800, 0x20, 0xdfff, 0x5d, 0x5b, 0xd800, 0x20, 0xdfff, 0x20, 0x78, 0x5d
        ]
    );
}

#[test]
fn session_jsonl_retains_message_summary_and_custom_units_on_reload() {
    for record in [
        r#"{"type":"message","id":"m","parentId":null,"timestamp":"2026-10-21T00:00:00Z","message":{"role":"toolResult","toolCallId":"c","toolName":"probe","content":[{"type":"text","text":"\ud800"}],"details":{"\udfff":"\ud800"},"isError":false,"timestamp":1}}"#,
        r#"{"type":"custom","id":"c","parentId":null,"timestamp":"2026-10-21T00:00:00Z","customType":"probe","data":{"\ud800":"\udfff"}}"#,
        r#"{"type":"compaction","id":"s","parentId":null,"timestamp":"2026-10-21T00:00:00Z","summary":"\ud800","firstKeptEntryId":"m","tokensBefore":4,"details":{"path":"\udfff"}}"#,
    ] {
        let header =
            r#"{"type":"session","version":3,"id":"lossless","timestamp":"2026-10-21T00:00:00Z","cwd":"/tmp"}"#;
        let parsed = parse_session_entries(&format!("{header}\n{record}\n"));
        let entry = parsed
            .iter()
            .find_map(|entry| match entry {
                FileEntry::Entry(entry) => Some(entry),
                _ => None,
            })
            .expect("valid record must not be discarded");
        assert!(
            matches!(entry, SessionEntry::Known(_)),
            "known record must remain typed"
        );
        let wire = serialize_session_entry(entry);
        assert_eq!(parse_lossless(&wire).unwrap(), parse_lossless(record).unwrap());
        let reloaded: SessionEntry = serde_json::from_str(&wire).unwrap();
        assert_eq!(&reloaded, entry);
        if let Some(message) = reloaded.as_message() {
            assert!(matches!(message.message.as_ref(), AgentMessage::ToolResult(_)));
        }
    }
}
