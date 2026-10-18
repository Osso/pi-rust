use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Combined cancellation token plus the bridge tasks that propagate aborts.
pub struct CombinedAbortSignal {
    signal: Option<CancellationToken>,
    tasks: Vec<JoinHandle<()>>,
}

impl CombinedAbortSignal {
    pub fn signal(&self) -> Option<&CancellationToken> {
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
pub fn combine_abort_signals(signals: &[Option<CancellationToken>]) -> CombinedAbortSignal {
    let active_signals: Vec<&CancellationToken> = signals.iter().flatten().collect();

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

fn combine_into_child_token(signals: &[&CancellationToken]) -> CombinedAbortSignal {
    let combined = CancellationToken::new();
    let mut tasks = Vec::new();

    for signal in signals {
        if signal.is_cancelled() {
            combined.cancel();
            break;
        }
        let combined = combined.clone();
        let signal = (*signal).clone();
        tasks.push(tokio::spawn(async move {
            signal.cancelled().await;
            combined.cancel();
        }));
    }

    CombinedAbortSignal {
        signal: Some(combined),
        tasks,
    }
}
