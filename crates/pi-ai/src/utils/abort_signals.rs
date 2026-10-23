use pi_js_compat::abort::{AbortController, AbortSignal};
use tokio::task::JoinHandle;

/// Combined cancellation token plus the bridge tasks that propagate aborts.
pub struct CombinedAbortSignal {
    signal: Option<AbortSignal>,
    tasks: Vec<JoinHandle<()>>,
}

impl CombinedAbortSignal {
    pub fn signal(&self) -> Option<&AbortSignal> {
        self.signal.as_ref()
    }

    /// Stops propagating aborts from the source signals.
    pub fn cleanup(&self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Drop for CombinedAbortSignal {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Combines optional signals into one token that aborts when any source aborts.
///
/// More than one source requires a Tokio runtime because each source is
/// observed by a bridge task.
pub fn combine_abort_signals(signals: &[Option<AbortSignal>]) -> CombinedAbortSignal {
    let active_signals: Vec<&AbortSignal> = signals.iter().flatten().collect();

    match active_signals.as_slice() {
        [] => CombinedAbortSignal {
            signal: None,
            tasks: Vec::new(),
        },
        [signal] => CombinedAbortSignal {
            signal: Some((*signal).clone()),
            tasks: Vec::new(),
        },
        _ => combine_into_child_token(&active_signals),
    }
}

fn combine_into_child_token(signals: &[&AbortSignal]) -> CombinedAbortSignal {
    let combined = AbortController::new();
    let mut tasks = Vec::new();

    for signal in signals {
        if signal.aborted() {
            combined.abort(None);
            break;
        }
        let combined = combined.clone();
        let signal = (*signal).clone();
        tasks.push(tokio::spawn(async move {
            signal.wait().await;
            combined.abort(None);
        }));
    }

    CombinedAbortSignal {
        signal: Some(combined.signal()),
        tasks,
    }
}
