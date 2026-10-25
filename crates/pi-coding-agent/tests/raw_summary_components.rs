use crate::shared_oracle as oracle;
use pi_agent_core::harness::messages::{BranchSummaryMessage, BranchSummaryRole};
use pi_coding_agent::ParsedSkillBlock;
use pi_coding_agent::modes::interactive::components::{
    branch_summary_message::BranchSummaryMessageComponent, skill_invocation_message::SkillInvocationMessageComponent,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme, set_theme_instance};
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::{Component, MarkdownTheme};
use serde_json::json;
use std::{rc::Rc, sync::Arc};
struct Plain;
macro_rules! plain {($($name:ident),*) => {$ (fn $name(&self,text:&JsString)->JsString{text.clone()})*};}
impl MarkdownTheme for Plain {
    plain!(
        heading,
        link,
        link_url,
        code,
        code_block,
        code_block_border,
        quote,
        quote_border,
        hr,
        list_bullet,
        bold,
        italic,
        strikethrough,
        underline
    );
}
#[test]
fn raw_branch_and_skill_text_reaches_actual_markdown_and_box_without_encoding() {
    let foreground =
        ["customMessageLabel", "customMessageText", "dim"].map(|name| (JsString::from(name), ColorValue::Index(0.0)));
    let theme = Arc::new(
        Theme::new(
            &foreground,
            &[(JsString::from("customMessageBg"), ColorValue::Index(0.0))],
            ColorMode::Color256,
            None,
            None,
        )
        .unwrap(),
    );
    set_theme_instance(theme.clone());
    let mut oracle = oracle::TsOracle::spawn("raw-coding-tui-oracle.ts", "packages/coding-agent/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let mut branch = BranchSummaryMessageComponent::new(
            BranchSummaryMessage {
                role: BranchSummaryRole::BranchSummary,
                summary: raw.clone(),
                from_id: "root".into(),
                timestamp: 0.0,
            },
            Some(Rc::new(Plain)),
        );
        let mut skill = SkillInvocationMessageComponent::new(
            ParsedSkillBlock {
                name: raw.clone(),
                location: "path".into(),
                content: raw.clone(),
                user_message: None,
            },
            Rc::new(Plain),
            theme.clone(),
        );
        let mut snapshots = Vec::new();
        for width in [4, 12, 40] {
            for expanded in [false, true] {
                branch.set_expanded(expanded);
                skill.set_expanded(expanded);
                snapshots.push((plain_lines(branch.render(width)), plain_lines(skill.render(width))));
                branch.invalidate();
                skill.invalidate();
                snapshots.push((plain_lines(branch.render(width)), plain_lines(skill.render(width))));
            }
        }
        let expected = oracle.request(&json!({"operation":"summaries","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let actual = serde_json::to_string(&snapshots).unwrap();
        let actual = parse_lossless(actual).unwrap();
        let expected = parse_lossless(expected["json"].as_str().unwrap()).unwrap();
        let actual = actual.as_array().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            let actual = actual.as_array().unwrap();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len());
            for (component, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(actual, expected, "raw={raw:?} snapshot={index} component={component}");
            }
        }
    }
}

fn plain_lines(lines: Vec<JsString>) -> Vec<JsString> {
    lines.into_iter().map(pi_tui::utils::strip_terminal_sequences).collect()
}
