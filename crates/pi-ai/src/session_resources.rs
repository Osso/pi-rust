use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub type SessionResourceCleanupErrorBox = Box<dyn Error + Send + Sync>;
pub type SessionResourceCleanup = Box<dyn Fn(Option<&str>) -> Result<(), SessionResourceCleanupErrorBox> + Send + Sync>;

type SharedCleanup = Arc<dyn Fn(Option<&str>) -> Result<(), SessionResourceCleanupErrorBox> + Send + Sync>;

static REGISTRY: OnceLock<Mutex<Vec<(u64, SharedCleanup)>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct SessionResourceCleanupError {
    pub errors: Vec<SessionResourceCleanupErrorBox>,
}

impl fmt::Display for SessionResourceCleanupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Failed to cleanup session resources")
    }
}

impl Error for SessionResourceCleanupError {}

pub struct SessionResourceCleanupRegistration {
    id: u64,
}

impl SessionResourceCleanupRegistration {
    pub fn unregister(self) {
        let registry = registry();
        let mut cleanups = registry.lock().expect("session resource registry mutex poisoned");
        cleanups.retain(|(id, _)| *id != self.id);
    }
}

pub fn register_session_resource_cleanup(cleanup: SessionResourceCleanup) -> SessionResourceCleanupRegistration {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let registry = registry();
    let mut cleanups = registry.lock().expect("session resource registry mutex poisoned");
    cleanups.push((id, Arc::from(cleanup)));
    SessionResourceCleanupRegistration { id }
}

pub fn cleanup_session_resources(session_id: Option<&str>) -> Result<(), SessionResourceCleanupError> {
    let snapshot: Vec<SharedCleanup> = {
        let registry = registry();
        let cleanups = registry.lock().expect("session resource registry mutex poisoned");
        cleanups.iter().map(|(_, cleanup)| Arc::clone(cleanup)).collect()
    };

    let errors: Vec<SessionResourceCleanupErrorBox> = snapshot
        .iter()
        .filter_map(|cleanup| cleanup(session_id).err())
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(SessionResourceCleanupError { errors })
    }
}

fn registry() -> &'static Mutex<Vec<(u64, SharedCleanup)>> {
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}
