use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use std::borrow::Cow;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone)]
pub enum SessionError {
    SessionAlreadyActive,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for SessionError {}

#[derive(Clone)]
pub(crate) struct MemorySession {
    /// This session will be discarded/disregarded after this timestamp.
    /// None means that this session is currently active.
    pub end_timestamp: Option<DateTime<Utc>>,
    _strong: Arc<()>,
}

#[async_trait]
impl Session for MemorySession {
    async fn clear(&mut self) -> Result<(), SessionError> {
        self.end_timestamp = None;
        Ok(())
    }

    async fn set_endtimestamp(
        &mut self,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), SessionError> {
        self.end_timestamp = timestamp;
        Ok(())
    }

    async fn get_endtimestamp(&self) -> Result<Option<Cow<DateTime<Utc>>>, SessionError> {
        Ok(self.end_timestamp.as_ref().map(Cow::Borrowed))
    }
}

pub(crate) struct MemorySessionProvider {
    sessions: DashMap<String, Weak<()>>,
}

impl MemorySessionProvider {
    pub(crate) fn new() -> MemorySessionProvider {
        MemorySessionProvider {
            sessions: DashMap::new(),
        }
    }
}

#[async_trait]
impl SessionProvider for MemorySessionProvider {
    type S = MemorySession;

    async fn get(&self, client_id: &str) -> Result<Self::S, SessionError> {
        match self.sessions.entry(client_id.to_owned()) {
            Entry::Occupied(occupied) if occupied.get().strong_count() > 0 => {
                Err(SessionError::SessionAlreadyActive)
            }
            Entry::Occupied(occupied) => {
                let strong = Arc::new(());
                let weak = Arc::downgrade(&strong);
                occupied.replace_entry(weak);
                Ok(MemorySession {
                    end_timestamp: None,
                    _strong: strong,
                })
            }
            Entry::Vacant(vacant) => {
                let strong = Arc::new(());
                let weak = Arc::downgrade(&strong);
                vacant.insert(weak);
                Ok(MemorySession {
                    end_timestamp: None,
                    _strong: strong,
                })
            }
        }
    }
}

#[async_trait]
pub trait Session: Send + Sync {
    async fn clear(&mut self) -> Result<(), SessionError>;
    async fn set_endtimestamp(
        &mut self,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), SessionError>;
    async fn get_endtimestamp(&self) -> Result<Option<Cow<DateTime<Utc>>>, SessionError>;
}

#[async_trait]
pub trait SessionProvider: Send + Sync {
    type S: Session;
    async fn get(&self, client_id: &str) -> Result<Self::S, SessionError>;
}
