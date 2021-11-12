use async_trait::async_trait;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub(crate) enum SessionError {
    AlreadySet,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for SessionError {}

#[async_trait]
pub trait Session: Send + Sync {
    async fn set_discarded_instant(&mut self, unix_time: Option<u64>);
}

#[async_trait]
pub trait SessionStorage: Send + Sync {
    type S: Session;
    async fn get(&self, client_id: &str) -> Option<Self::S>;
    async fn insert(&mut self, client_id: &str, session: Self::S) -> Result<(), Box<dyn Error>>;
}

pub struct MemorySession {
    discarded_at: Option<u64>,
}

#[async_trait]
impl Session for Arc<RwLock<MemorySession>> {
    async fn set_discarded_instant(&mut self, unix_time: Option<u64>) {
        self.write().await.discarded_at = unix_time;
    }
}

pub struct MemorySessionStorage {
    sessions: dashmap::DashMap<String, Arc<RwLock<MemorySession>>>,
}

impl MemorySessionStorage {
    pub fn new() -> Self {
        MemorySessionStorage {
            sessions: dashmap::DashMap::new(),
        }
    }
}

#[async_trait]
impl SessionStorage for MemorySessionStorage {
    type S = Arc<RwLock<MemorySession>>;

    async fn get(&self, client_id: &str) -> Option<Self::S> {
        self.sessions.get(client_id).map(|s| s.clone())
    }

    async fn insert(&mut self, client_id: &str, session: Self::S) -> Result<(), Box<dyn Error>> {
        match self.sessions.entry(client_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(_) => Err(Box::new(SessionError::AlreadySet)),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(session);
                Ok(())
            }
        }
    }
}
