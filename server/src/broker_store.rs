use crate::topic_filter::TopicFilter;
use dashmap::DashMap;
use mqtt_protocol::Publish;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ClusterStoreError {}

impl Display for ClusterStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for ClusterStoreError {}

#[async_trait::async_trait]
pub trait ClusterStore: Send + Sync + Clone {
    async fn replace_retained(
        &mut self,
        topic_name: &str,
        publish: Publish,
    ) -> Result<(), ClusterStoreError>;
    async fn remove_retained(&mut self, topic_name: &str) -> Result<(), ClusterStoreError>;
    async fn get_retained(
        &mut self,
        topic_filter: &TopicFilter,
    ) -> Result<Vec<Publish>, ClusterStoreError>;
}

#[derive(Debug, Clone)]
pub struct MemoryClusterStore {
    retained: Arc<DashMap<String, Publish>>,
}

impl MemoryClusterStore {
    pub fn new() -> Self {
        MemoryClusterStore {
            retained: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl ClusterStore for MemoryClusterStore {
    async fn replace_retained(
        &mut self,
        topic_name: &str,
        publish: Publish,
    ) -> Result<(), ClusterStoreError> {
        self.retained
            .entry(topic_name.to_string())
            .and_modify(|existing| {
                _ = std::mem::replace(existing, publish.clone());
            })
            .or_insert(publish);
        Ok(())
    }

    async fn remove_retained(&mut self, topic_name: &str) -> Result<(), ClusterStoreError> {
        self.retained.remove(topic_name);
        Ok(())
    }

    async fn get_retained(
        &mut self,
        topic_filter: &TopicFilter,
    ) -> Result<Vec<Publish>, ClusterStoreError> {
        println!("get_retained: {}", self.retained.len());
        Ok(self
            .retained
            .iter()
            .filter(|i| topic_filter.matches(&i.value().topic_name))
            .map(|i| i.value().clone())
            .collect())
    }
}
