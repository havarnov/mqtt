use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum SessionError {
    ETagMismatch,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for SessionError {}

#[derive(Clone)]
pub struct Session {
    e_tag: Option<String>,
    pub discarded_at: Option<u64>,
}

#[async_trait]
pub trait SessionStorage: Send + Sync {
    async fn get(&self, client_id: &str) -> Option<Session>;
    async fn upsert(&self, client_id: &str, session: Session) -> Result<(), SessionError>;
}

pub struct MemorySessionStorage {
    sessions: dashmap::DashMap<String, Session>,
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
    async fn get(&self, client_id: &str) -> Option<Session> {
        self.sessions.get(client_id).map(|s| s.clone())
    }

    async fn upsert(&self, client_id: &str, session: Session) -> Result<(), SessionError> {
        match self.sessions.entry(client_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                if entry.get().e_tag != session.e_tag {
                    Err(SessionError::ETagMismatch)
                } else {
                    entry.insert(session);
                    Ok(())
                }
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(session);
                Ok(())
            }
        }
    }
}
