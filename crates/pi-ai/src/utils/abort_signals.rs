//! Port of frozen `utils/abort-signals.ts`: synchronous listeners with explicit cleanup.
use pi_js_compat::abort::{AbortController, AbortSignal, ListenerId};

/// Dropping the wrapper does not detach listeners from a retained returned signal.
pub struct CombinedAbortSignal {
    signal: Option<AbortSignal>,
    listeners: Vec<(AbortSignal, ListenerId)>,
}

impl CombinedAbortSignal {
    pub fn signal(&self) -> Option<&AbortSignal> {
        self.signal.as_ref()
    }

    pub fn cleanup(&self) {
        for (signal, listener) in &self.listeners {
            signal.remove_listener(listener);
        }
    }
}

pub fn combine_abort_signals(signals: &[Option<AbortSignal>]) -> CombinedAbortSignal {
    let active: Vec<&AbortSignal> = signals.iter().flatten().collect();
    match active.as_slice() {
        [] => CombinedAbortSignal {
            signal: None,
            listeners: Vec::new(),
        },
        [signal] => CombinedAbortSignal {
            signal: Some((*signal).clone()),
            listeners: Vec::new(),
        },
        _ => combine_with_listeners(&active),
    }
}

fn combine_with_listeners(signals: &[&AbortSignal]) -> CombinedAbortSignal {
    let controller = AbortController::new();
    let mut listeners = Vec::new();
    for signal in signals {
        if signal.aborted() {
            controller.abort(signal.reason());
            break;
        }
        let target = controller.clone();
        let listener = signal.add_listener(move |reason| {
            if !target.signal().aborted() {
                target.abort(Some(reason.clone()));
            }
        });
        listeners.push(((*signal).clone(), listener));
        // Native callers can abort from another thread during check/register; JS runs them atomically.
        if signal.aborted() {
            controller.abort(signal.reason());
        }
    }
    CombinedAbortSignal {
        signal: Some(controller.signal()),
        listeners,
    }
}
