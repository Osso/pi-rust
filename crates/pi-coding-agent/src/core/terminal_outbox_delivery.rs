//! Terminal outbox projection delivery (`core/terminal-outbox-delivery.ts`).

use std::sync::Arc;

use crate::core::detached_job_cleanup::run_detached_job_artifact_cleanup;
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore};
use crate::core::session_control_db::{
    ClaimMultiAgentTerminalOutboxOptions, MultiAgentTerminalOutboxRecord, claim_multi_agent_terminal_outbox,
    deliver_multi_agent_terminal_outbox, fail_multi_agent_terminal_outbox, read_multi_agent_agent,
};
use crate::core::sqlite::ControlDbError;

const TERMINAL_OUTBOX_CLAIM_LEASE_MS: i64 = 30_000;
const TERMINAL_OUTBOX_MAX_ATTEMPTS: i64 = 5;
/// `TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS`.
pub const TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS: i64 = 60 * 60 * 1_000;
const TERMINAL_OUTBOX_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// `DeliverTerminalOutboxOptions`.
pub struct DeliverTerminalOutboxOptions {
    pub artifact_root: Option<String>,
    pub claim_id: String,
    pub control_db_path: String,
    pub now: Arc<dyn Fn() -> String + Send + Sync>,
    pub store: Arc<MultiAgentStore>,
}

/// `isTerminalOutboxCleanupDue`.
pub fn is_terminal_outbox_cleanup_due(last_cleanup_at: Option<i64>, now: i64) -> bool {
    last_cleanup_at.is_none_or(|last_cleanup_at| now - last_cleanup_at >= TERMINAL_OUTBOX_CLEANUP_INTERVAL_MS)
}

/// `terminalOutboxRetentionThreshold`.
pub fn terminal_outbox_retention_threshold(now: i64) -> String {
    crate::utils::iso_time::format_iso_millis(now - TERMINAL_OUTBOX_RETENTION_MS)
}

/// `deliverTerminalOutboxProjections`.
pub fn deliver_terminal_outbox_projections(options: &DeliverTerminalOutboxOptions) -> Result<u32, ControlDbError> {
    let Some(persistence) = options.store.get_persistence_target() else {
        return Ok(0);
    };
    let mut delivered = 0_u32;
    let result = deliver_claimed_projections(options, &persistence.session_path, &mut delivered);
    if delivered > 0 {
        let now = (options.now)();
        let now_millis = pi_js_compat::date::date_parse_millis(&now).unwrap_or_default();
        run_detached_job_artifact_cleanup(
            &options.control_db_path,
            options.artifact_root.as_deref(),
            Some(now_millis),
        );
    }
    result.map(|()| delivered)
}

fn deliver_claimed_projections(
    options: &DeliverTerminalOutboxOptions,
    session_path: &str,
    delivered: &mut u32,
) -> Result<(), ControlDbError> {
    loop {
        let now_iso = (options.now)();
        let stale_claim_before = pi_js_compat::date::date_parse_millis(&now_iso)
            .map(|now| crate::utils::iso_time::format_iso_millis(now - TERMINAL_OUTBOX_CLAIM_LEASE_MS));
        let record = claim_multi_agent_terminal_outbox(
            &options.control_db_path,
            &options.claim_id,
            &now_iso,
            &ClaimMultiAgentTerminalOutboxOptions {
                max_attempts: Some(TERMINAL_OUTBOX_MAX_ATTEMPTS),
                session_path: Some(session_path.to_owned()),
                stale_claim_before,
            },
        )?;
        let Some(record) = record else {
            return Ok(());
        };
        deliver_terminal_outbox_projection(options, &record)?;
        *delivered += 1;
    }
}

fn deliver_terminal_outbox_projection(
    options: &DeliverTerminalOutboxOptions,
    record: &MultiAgentTerminalOutboxRecord,
) -> Result<(), ControlDbError> {
    match project_terminal_outbox(options, record) {
        Ok(()) => Ok(()),
        Err(error) => {
            fail_multi_agent_terminal_outbox(
                &options.control_db_path,
                record,
                error.message(),
                &(options.now)(),
                TERMINAL_OUTBOX_MAX_ATTEMPTS,
            )?;
            Err(error)
        }
    }
}

fn project_terminal_outbox(
    options: &DeliverTerminalOutboxOptions,
    record: &MultiAgentTerminalOutboxRecord,
) -> Result<(), ControlDbError> {
    let agent = read_multi_agent_agent(&options.control_db_path, &record.session_path, &record.agent_id)?;
    let Some(agent) = agent else {
        return Err(mismatch_error(record));
    };
    if agent.revision != record.terminal_revision || !is_terminal_lifecycle(&agent) {
        return Err(mismatch_error(record));
    }
    options.store.publish_terminal_outbox_snapshot(&agent);
    if !deliver_multi_agent_terminal_outbox(&options.control_db_path, record, &(options.now)())? {
        return Err(ControlDbError::new(format!(
            "Could not acknowledge terminal outbox projection for {}",
            record.agent_id
        )));
    }
    Ok(())
}

fn mismatch_error(record: &MultiAgentTerminalOutboxRecord) -> ControlDbError {
    ControlDbError::new(format!(
        "Terminal outbox projection does not match agent {} revision {}",
        record.agent_id, record.terminal_revision
    ))
}

fn is_terminal_lifecycle(agent: &AgentSnapshot) -> bool {
    matches!(
        agent.lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}
