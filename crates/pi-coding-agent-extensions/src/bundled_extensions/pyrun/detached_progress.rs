//! Port of `extensions/pyrun/src/detached-progress.ts`.

use super::runner::CanonicalPyrunProgressUpdate;
use crate::core::extensions::types::HandlerError;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

const DENSE_CONSOLE_RECORD_THRESHOLD: usize = 100;
pub type ProgressReporter = Arc<dyn Fn(CanonicalPyrunProgressUpdate) -> Result<(), HandlerError> + Send + Sync>;

#[derive(Default)]
struct PendingProgress {
    updates: Vec<CanonicalPyrunProgressUpdate>,
    stream: Option<JsValue>,
    dense_text: Vec<JsString>,
    dense_update: Option<CanonicalPyrunProgressUpdate>,
    timer: Option<JoinHandle<()>>,
}

impl PendingProgress {
    fn reset(&mut self) {
        self.updates.clear();
        self.stream = None;
        self.dense_text.clear();
        self.dense_update = None;
    }

    fn clear_timer(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
    }

    fn flush(&mut self, report: &ProgressReporter) -> Result<(), HandlerError> {
        self.clear_timer();
        if let Some(update) = &self.dense_update {
            report(update.with_text(JsString::join(self.dense_text.iter(), "")))?;
        } else {
            for update in &self.updates {
                report(update.clone())?;
            }
        }
        self.reset();
        Ok(())
    }

    fn append_console(&mut self, update: CanonicalPyrunProgressUpdate) {
        self.stream = self.stream.take().or_else(|| update.raw.get("stream").cloned());
        if self.dense_update.is_some() {
            self.dense_text.push(update.text().cloned().unwrap_or_default());
            self.dense_update = Some(update);
        } else {
            self.updates.push(update.clone());
            if self.updates.len() >= DENSE_CONSOLE_RECORD_THRESHOLD {
                self.dense_text = self
                    .updates
                    .iter()
                    .filter_map(|update| update.text().cloned())
                    .collect();
                self.dense_update = Some(update);
                self.updates.clear();
            }
        }
    }
}

pub fn is_console_progress_update(update: &CanonicalPyrunProgressUpdate) -> bool {
    update.message_type() == "console" && update.text().is_some()
}

pub struct ArtifactProgressAccumulator {
    pending: Arc<Mutex<PendingProgress>>,
    report: ProgressReporter,
    flush_delay_ms: u64,
}

impl ArtifactProgressAccumulator {
    pub fn new(report: ProgressReporter, flush_delay_ms: u64) -> Self {
        Self {
            pending: Arc::new(Mutex::new(PendingProgress::default())),
            report,
            flush_delay_ms,
        }
    }

    pub fn consume(&mut self, records: &[JsValue]) -> Result<Option<JsValue>, HandlerError> {
        let mut result = None;
        for record in records {
            if record["kind"].as_string().is_some_and(|kind| kind == "progress") {
                self.consume_progress(CanonicalPyrunProgressUpdate {
                    raw: record["update"].clone(),
                })?;
            } else {
                self.pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .flush(&self.report)?;
                if record["kind"].as_string().is_some_and(|kind| kind == "result") {
                    result = record.get("result").cloned().or(result);
                }
            }
        }
        Ok(result)
    }

    pub fn close(&mut self) {
        let mut pending = self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        pending.clear_timer();
        pending.reset();
    }

    fn consume_progress(&self, update: CanonicalPyrunProgressUpdate) -> Result<(), HandlerError> {
        let mut pending = self.pending.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !is_console_progress_update(&update) {
            pending.flush(&self.report)?;
            return (self.report)(update);
        }
        if pending.stream.is_some() && pending.stream.as_ref() != update.raw.get("stream") {
            pending.flush(&self.report)?;
        }
        pending.append_console(update);
        if pending.timer.is_none() {
            pending.timer = Some(self.spawn_flush_timer());
        }
        Ok(())
    }

    fn spawn_flush_timer(&self) -> JoinHandle<()> {
        let state = self.pending.clone();
        let report = self.report.clone();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(self.flush_delay_ms);
        tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.timer.take();
            if let Err(error) = state.flush(&report) {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message);
            }
        })
    }
}
