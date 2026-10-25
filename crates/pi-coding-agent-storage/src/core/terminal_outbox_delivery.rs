//! Terminal outbox projection delivery (`core/terminal-outbox-delivery.ts`).

use std::sync::Arc;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

use crate::core::detached_job_cleanup::run_detached_job_artifact_cleanup;
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore};
use crate::core::session_control_db::{
    ClaimMultiAgentTerminalOutboxOptions, MultiAgentTerminalOutboxRecord, claim_multi_agent_terminal_outbox,
    deliver_multi_agent_terminal_outbox, fail_multi_agent_terminal_outbox, read_multi_agent_agent,
};

const TERMINAL_OUTBOX_CLAIM_LEASE_MS: i64 = 30_000;
const TERMINAL_OUTBOX_MAX_ATTEMPTS: i64 = 5;
/// `TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS`.
pub const TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS: i64 = 60 * 60 * 1_000;
const TERMINAL_OUTBOX_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// `DeliverTerminalOutboxOptions`.
pub struct DeliverTerminalOutboxOptions {
    pub artifact_root: Option<JsString>,
    pub claim_id: JsString,
    pub control_db_path: JsString,
    pub now: Arc<dyn Fn() -> Result<JsString, CallbackError> + Send + Sync>,
    pub store: Arc<MultiAgentStore>,
}

/// `isTerminalOutboxCleanupDue`.
pub fn is_terminal_outbox_cleanup_due(last_cleanup_at: Option<f64>, now: f64) -> bool {
    last_cleanup_at.is_none_or(|last_cleanup_at| now - last_cleanup_at >= TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS as f64)
}

/// `terminalOutboxRetentionThreshold`.
pub fn terminal_outbox_retention_threshold(now: f64) -> Result<String, Box<NativeNodeError>> {
    iso_timestamp(now - TERMINAL_OUTBOX_RETENTION_MS as f64)
}

fn iso_timestamp(millis: f64) -> Result<String, Box<NativeNodeError>> {
    pi_js_compat::date::to_iso_string_value(millis).map_err(|error| {
        Box::new(NativeNodeError::from_constructor(
            NativeErrorClass::RangeError,
            error.to_string(),
        ))
    })
}

/// `deliverTerminalOutboxProjections`.
pub fn deliver_terminal_outbox_projections(options: &DeliverTerminalOutboxOptions) -> Result<f64, CallbackError> {
    let Some(persistence) = options.store.get_persistence_target() else {
        return Ok(0.0);
    };
    let mut delivered = 0.0;
    let result = deliver_claimed_projections(options, &persistence.session_path, &mut delivered);
    if delivered > 0.0 {
        let now = (options.now)()?;
        let now_millis = pi_js_compat::date::date_parse_utf16(now.units());
        run_detached_job_artifact_cleanup(
            &options.control_db_path,
            options.artifact_root.as_ref(),
            Some(now_millis),
        )?;
    }
    result.map(|()| delivered)
}

fn deliver_claimed_projections(
    options: &DeliverTerminalOutboxOptions,
    session_path: &JsString,
    delivered: &mut f64,
) -> Result<(), CallbackError> {
    loop {
        let now_iso = (options.now)()?;
        let stale_claim_before = iso_timestamp(
            pi_js_compat::date::date_parse_utf16(now_iso.units()) - TERMINAL_OUTBOX_CLAIM_LEASE_MS as f64,
        )
        .map_err(|error| CallbackError::from(*error))?;
        let record = claim_multi_agent_terminal_outbox(
            &options.control_db_path,
            &options.claim_id,
            &pi_js_compat::js_json::encode_node_utf8(&now_iso),
            &ClaimMultiAgentTerminalOutboxOptions {
                max_attempts: Some(TERMINAL_OUTBOX_MAX_ATTEMPTS),
                session_path: Some(session_path.clone()),
                stale_claim_before: Some(stale_claim_before),
            },
        )?;
        let Some(record) = record else {
            return Ok(());
        };
        deliver_terminal_outbox_projection(options, &record)?;
        *delivered += 1.0;
    }
}

fn deliver_terminal_outbox_projection(
    options: &DeliverTerminalOutboxOptions,
    record: &MultiAgentTerminalOutboxRecord,
) -> Result<(), CallbackError> {
    match project_terminal_outbox(options, record) {
        Ok(()) => Ok(()),
        Err(error) => {
            fail_multi_agent_terminal_outbox(
                &options.control_db_path,
                record,
                &error.error_or_string_message()?,
                &pi_js_compat::js_json::encode_node_utf8(&(options.now)()?),
                TERMINAL_OUTBOX_MAX_ATTEMPTS,
            )?;
            Err(error)
        }
    }
}

fn project_terminal_outbox(
    options: &DeliverTerminalOutboxOptions,
    record: &MultiAgentTerminalOutboxRecord,
) -> Result<(), CallbackError> {
    let agent = read_multi_agent_agent(&options.control_db_path, &record.session_path, &record.agent_id)?;
    let Some(agent) = agent else {
        return Err(mismatch_error(record));
    };
    if agent.revision != record.terminal_revision || !is_terminal_lifecycle(&agent) {
        return Err(mismatch_error(record));
    }
    options
        .store
        .publish_terminal_outbox_snapshot(&agent)
        .map_err(|error| CallbackError::from_error(error.message.clone(), error))?;
    if !deliver_multi_agent_terminal_outbox(&options.control_db_path, record)? {
        return Err(CallbackError::new(js_format!(
            "Could not acknowledge terminal outbox projection for {}",
            record.agent_id
        )));
    }
    Ok(())
}

fn mismatch_error(record: &MultiAgentTerminalOutboxRecord) -> CallbackError {
    CallbackError::new(js_format!(
        "Terminal outbox projection does not match agent {} revision {}",
        record.agent_id,
        record.terminal_revision.to_string()
    ))
}

fn is_terminal_lifecycle(agent: &AgentSnapshot) -> bool {
    matches!(
        agent.lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}
