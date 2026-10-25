//! Frozen agents-status tree ordering, formatting and widget lifetime.
#[path = "support/oracle.rs"]
mod oracle;
#[path = "agents_status_widget.rs"]
mod widget;
use pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent;
use pi_coding_agent::bundled_extensions::agents_status::{
    collect_agent_rows, describe_agent_kind, render_agents_status,
};
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentSnapshot};
use pi_coding_agent::modes::interactive::theme::{ThemeColor, get_theme_by_name};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

fn agent(id: &str, parent: Option<&str>, lifecycle: AgentLifecycle, updated: &str) -> AgentSnapshot {
    let mut agent = empty_agent(id);
    agent.parent_id = parent.map(JsString::from);
    agent.display_name = JsString::from("  名称 ") + id + "\nname";
    agent.agent_type = "explore".into();
    agent.lifecycle = lifecycle;
    agent.created_at = "2026-10-25T00:00:00.000Z".into();
    agent.updated_at = updated.into();
    agent
}

#[test]
fn status_tree_and_rendering_match_frozen_for_widths_lifecycles_and_linger() {
    let theme = get_theme_by_name(&"dark".into()).unwrap();
    let colors = json!({
        "accent":theme.get_fg_ansi(ThemeColor::Accent),"warning":theme.get_fg_ansi(ThemeColor::Warning),
        "muted":theme.get_fg_ansi(ThemeColor::Muted),"success":theme.get_fg_ansi(ThemeColor::Success),
        "error":theme.get_fg_ansi(ThemeColor::Error),"dim":theme.get_fg_ansi(ThemeColor::Dim),
    });
    let mut ts = oracle::TsOracle::spawn(
        "agents-status-oracle.ts",
        "packages/coding-agent/extensions/agents-status/src/index.ts",
    );
    let mut agents = vec![
        agent("root", None, AgentLifecycle::Running, "2026-10-25T00:00:15.000Z"),
        agent(
            "child",
            Some("root"),
            AgentLifecycle::WaitingForInput,
            "2026-10-25T00:00:15.000Z",
        ),
        agent(
            "grandchild",
            Some("child"),
            AgentLifecycle::SteeringPending,
            "2026-10-25T00:00:15.000Z",
        ),
        agent(
            "orphan",
            Some("expired"),
            AgentLifecycle::Running,
            "2026-10-25T00:00:15.000Z",
        ),
        agent("failed", None, AgentLifecycle::Failed, "2026-10-25T00:00:15.000Z"),
        agent("aborted", None, AgentLifecycle::Aborted, "2026-10-25T00:00:15.000Z"),
        agent("done", None, AgentLifecycle::Completed, "2026-10-25T00:00:15.000Z"),
        agent("expired", None, AgentLifecycle::Completed, "2026-10-25T00:00:05.000Z"),
        agent(
            "cancelling",
            Some("root"),
            AgentLifecycle::Cancelling,
            "2026-10-25T00:00:16.000Z",
        ),
    ];
    agents[0].model = Some(
        pi_js_compat::js_json::from_value(
            &json!({"modelId":"wide-模型","providerId":"fixture","thinkingLevel":"high"}),
        )
        .unwrap(),
    );
    agents[0].current_activity = Some(pi_js_compat::js_json::from_value(&json!({"phase":"tool","toolCallId":"call1","toolName":"read","detail":"a\nb.rs","startedAt":"2026-10-25T00:00:15.000Z"})).unwrap());
    agents[2].current_activity = Some(
        pi_js_compat::js_json::from_value(&json!({"phase":"thinking","startedAt":"2026-10-25T00:00:14.000Z"})).unwrap(),
    );
    agents[4].error = Some(pi_js_compat::js_json::from_value(&json!({"message":"failed\nrequest"})).unwrap());
    let now = pi_js_compat::date::date_parse("2026-10-25T00:00:20.000Z");
    for values in [vec![], agents[..1].to_vec(), agents.clone(), agents[4..].to_vec()] {
        for width in [0, 1, 8, 24, 60, 120] {
            let source = ts.request(&json!({"id":1,"agents":values,"now":now,"width":width,"colors":colors}));
            let rows: Vec<JsValue> = collect_agent_rows(&values, now)
                .iter()
                .map(|row| json!({"id":row.agent.id,"depth":row.depth}))
                .collect();
            assert_eq!(source["rows"], json!(rows));
            assert_eq!(
                source["kinds"],
                json!(values.iter().map(describe_agent_kind).collect::<Vec<_>>())
            );
            assert_eq!(
                source["lines"],
                json!(render_agents_status(&values, width, now, &theme)),
                "width={width}"
            );
        }
    }
}
