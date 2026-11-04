use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// What the Bridge pill shows. "Connected" only after a reply with the right token
/// has actually arrived. A saved address and token alone never counts.
#[derive(Debug, Clone, PartialEq)]
pub enum Bridge {
    NotConnected,
    Listening,
    Connected,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub bridge: Bridge,
    pub last_reply_at: Option<SystemTime>,
    pub last_error: Option<String>,
    pub speaking: bool,
}

#[derive(Clone)]
pub struct Shared(Arc<Mutex<Snapshot>>);

impl Default for Shared {
    fn default() -> Self {
        Shared(Arc::new(Mutex::new(Snapshot {
            bridge: Bridge::NotConnected,
            last_reply_at: None,
            last_error: None,
            speaking: false,
        })))
    }
}

impl Shared {
    pub fn get(&self) -> Snapshot {
        self.0.lock().unwrap().clone()
    }
    pub fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        f(&mut self.0.lock().unwrap());
    }
}
