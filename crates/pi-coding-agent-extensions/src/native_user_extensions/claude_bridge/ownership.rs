//! The first serving extension instance owns provider registration until it clears.
use std::sync::{Arc, Mutex, OnceLock, Weak};

type RebuildHook = Arc<
    dyn Fn(&pi_js_compat::js_json::JsString, bool, &str) -> Result<(), pi_js_compat::callback_error::CallbackError>
        + Send
        + Sync,
>;
#[derive(Default)]
pub(super) struct Ownership {
    owner: Mutex<Weak<()>>,
    hooks: Mutex<Vec<(Weak<()>, RebuildHook)>>,
}
impl Ownership {
    pub fn subscribe(&self, instance: &Arc<()>, hook: RebuildHook) {
        let mut hooks = self.hooks.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        hooks.retain(|(instance, _)| instance.upgrade().is_some());
        hooks.push((Arc::downgrade(instance), hook));
    }
    pub fn mark_rebuild(
        &self,
        session: &pi_js_compat::js_json::JsString,
        rotate: bool,
        event: &str,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        let hooks = {
            let mut hooks = self.hooks.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            hooks.retain(|(instance, _)| instance.upgrade().is_some());
            hooks.iter().map(|(_, hook)| hook.clone()).collect::<Vec<_>>()
        };
        for hook in hooks {
            hook(session, rotate, event)?;
        }
        Ok(())
    }
    pub fn claim(&self, instance: &Arc<()>) -> bool {
        let mut owner = self.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if owner.upgrade().is_some() {
            return false;
        }
        *owner = Arc::downgrade(instance);
        true
    }
    pub fn release(&self, instance: &Arc<()>) -> bool {
        let mut owner = self.owner.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if owner.upgrade().is_some_and(|owner| Arc::ptr_eq(&owner, instance)) {
            *owner = Weak::new();
            true
        } else {
            false
        }
    }
}
pub(super) fn shared() -> Arc<Ownership> {
    static SHARED: OnceLock<Arc<Ownership>> = OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Ownership::default())).clone()
}
