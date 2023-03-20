use crate::topic_filter::TopicFilter;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use mqtt_protocol::{Publish, QoS};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone)]
pub enum SessionError {
    SessionAlreadyActive,
    AtLeastOnceError,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for SessionError {}

pub(crate) struct MemorySession {
    /// This session will be discarded/disregarded after this timestamp.
    /// None means that this session is currently active.
    end_timestamp: Option<DateTime<Utc>>,
    subscriptions: HashMap<String, ClientSubscription>,
    unsent_atleastonce: HashMap<u16, Publish>,
    unacked_atleastonce: HashMap<u16, Publish>,
    _strong: Arc<()>,
}

impl MemorySession {
    pub fn new(strong: Arc<()>) -> Self {
        MemorySession {
            end_timestamp: None,
            subscriptions: Default::default(),
            unsent_atleastonce: Default::default(),
            unacked_atleastonce: Default::default(),
            _strong: strong,
        }
    }
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

    async fn add_subscription(
        &mut self,
        topic: String,
        client_subscription: ClientSubscription,
    ) -> Result<AddSubscriptionResult, SessionError> {
        let mut res: AddSubscriptionResult = AddSubscriptionResult::New;
        self.subscriptions
            .entry(topic)
            .and_modify(|old| {
                old.maximum_qos = client_subscription.maximum_qos;
                old.retain_as_published = client_subscription.retain_as_published;
                old.topic_filter = client_subscription.topic_filter.clone();
                old.subscription_identifier = client_subscription.subscription_identifier;
                res = AddSubscriptionResult::Replaced;
            })
            .or_insert_with(|| {
                res = AddSubscriptionResult::New;
                client_subscription
            });

        Ok(res)
    }

    async fn remove_subscription(&mut self, topic: String) -> Result<Option<()>, SessionError> {
        Ok(self.subscriptions.remove(&topic).map(|_| ()))
    }

    async fn get_subscriptions(&self) -> Result<Vec<&ClientSubscription>, SessionError> {
        Ok(self.subscriptions.values().collect())
    }

    async fn add_unsent_atleastonce(
        &mut self,
        packet_identifier: u16,
        publish: Publish,
    ) -> Result<(), SessionError> {
        self.unsent_atleastonce.insert(packet_identifier, publish);
        Ok(())
    }

    async fn unacked_atleastonce(&mut self, packet_identifier: u16) -> Result<(), SessionError> {
        match self.unsent_atleastonce.remove(&packet_identifier) {
            Some(publish) => {
                self.unacked_atleastonce.insert(packet_identifier, publish);
                Ok(())
            }
            None => return Err(SessionError::AtLeastOnceError),
        }
    }

    async fn ack_atleastonce(&mut self, packet_identifier: u16) -> Result<(), SessionError> {
        match self.unacked_atleastonce.remove(&packet_identifier) {
            Some(_) => Ok(()),
            None => Err(SessionError::AtLeastOnceError),
        }
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
                Ok(MemorySession::new(strong))
            }
            Entry::Vacant(vacant) => {
                let strong = Arc::new(());
                let weak = Arc::downgrade(&strong);
                vacant.insert(weak);
                Ok(MemorySession::new(strong))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientSubscription {
    pub topic_filter: TopicFilter,
    pub subscription_identifier: Option<u32>,
    pub maximum_qos: QoS,
    pub retain_as_published: bool,
}

#[async_trait]
pub trait TransactionalSession: Send + Sync {
    type S: Session;
    async fn begin(&mut self) -> Result<(), Self::S>;
}

pub enum AddSubscriptionResult {
    New,
    Replaced,
}

#[async_trait]
pub trait Session: Send + Sync {
    async fn clear(&mut self) -> Result<(), SessionError>;
    async fn set_endtimestamp(
        &mut self,
        timestamp: Option<DateTime<Utc>>,
    ) -> Result<(), SessionError>;
    async fn get_endtimestamp(&self) -> Result<Option<Cow<DateTime<Utc>>>, SessionError>;
    async fn add_subscription(
        &mut self,
        topic: String,
        client_subscription: ClientSubscription,
    ) -> Result<AddSubscriptionResult, SessionError>;
    async fn remove_subscription(&mut self, topic: String) -> Result<Option<()>, SessionError>;
    async fn get_subscriptions(&self) -> Result<Vec<&ClientSubscription>, SessionError>;
    async fn add_unsent_atleastonce(
        &mut self,
        packet_identifier: u16,
        publish: Publish,
    ) -> Result<(), SessionError>;
    async fn unacked_atleastonce(&mut self, packet_identifier: u16) -> Result<(), SessionError>;
    async fn ack_atleastonce(&mut self, packet_identifier: u16) -> Result<(), SessionError>;
}

#[async_trait]
pub trait SessionProvider: Send + Sync {
    type S: Session;
    async fn get(&self, client_id: &str) -> Result<Self::S, SessionError>;
}
