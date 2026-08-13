use std::fmt::Debug;

use pi_agent_core::harness::AgentToolExecutionContext;
use pi_agent_core::harness::types::AgentToolExecutionContext as ModuleAgentToolExecutionContext;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn context_is_public_from_module_and_harness_root() {
    let module_context = ModuleAgentToolExecutionContext {
        started_at: 1.5,
        agent_id: Some("module-agent".to_owned()),
    };
    let root_context = AgentToolExecutionContext {
        started_at: 2.5,
        agent_id: Some("root-agent".to_owned()),
    };

    assert_eq!(module_context.started_at, 1.5);
    assert_eq!(module_context.agent_id.as_deref(), Some("module-agent"));
    assert_eq!(root_context.started_at, 2.5);
    assert_eq!(root_context.agent_id.as_deref(), Some("root-agent"));
    assert_debug(&module_context);
    assert_debug(&root_context);
}

#[test]
fn context_preserves_optional_agent_id_states() {
    let states = [None, Some(String::new()), Some("agent".to_owned())];

    for agent_id in states {
        let context = AgentToolExecutionContext {
            started_at: 0.0,
            agent_id: agent_id.clone(),
        };

        assert_eq!(context.agent_id, agent_id);
    }
}

#[test]
fn context_preserves_unicode_nul_and_newline_agent_ids() {
    let context = AgentToolExecutionContext {
        started_at: 0.0,
        agent_id: Some("代理🙂e\u{301}\0agent\nnext".to_owned()),
    };

    assert_eq!(context.agent_id.as_deref(), Some("代理🙂e\u{301}\0agent\nnext"));
}

#[test]
fn context_fields_are_publicly_mutable() {
    let mut context = AgentToolExecutionContext {
        started_at: 1.0,
        agent_id: Some("before".to_owned()),
    };

    context.started_at = -2.0;
    context.agent_id = None;

    assert_eq!(context.started_at, -2.0);
    assert_eq!(context.agent_id, None);
}

#[test]
fn context_preserves_started_at_binary64_bits() {
    let values = [
        0.0_f64,
        -0.0_f64,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MIN,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8_0000_0000_0001),
        f64::from_bits(0x7ff8_0000_0000_0002),
    ];

    for value in values {
        let context = AgentToolExecutionContext {
            started_at: value,
            agent_id: None,
        };

        assert_eq!(context.started_at.to_bits(), value.to_bits());
    }
}
