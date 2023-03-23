use mqtt_protocol::codec::MqttPacketEncoderError;
use mqtt_protocol::{Disconnect, DisconnectReason, MqttPacket, PubAck, PubAckReason, Publish, QoS};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::SendError;
use tokio::time::Instant;

use crate::broker_store::{ClusterStore, ClusterStoreError};
use crate::{ClientBroadcastMessage, MqttSinkStream};

pub(crate) struct PublishHandler<S: ClusterStore> {
    broker_store: S,
    clients_tx: broadcast::Sender<ClientBroadcastMessage>,
    max_topic_alias: u16,
}

#[derive(Debug)]
pub(crate) enum HandlePublishError {
    TopicAlias(String),
    BroadcastSend(SendError<ClientBroadcastMessage>),
    Framed(MqttPacketEncoderError),
    ClusterStore(ClusterStoreError),
}

impl From<SendError<ClientBroadcastMessage>> for HandlePublishError {
    fn from(error: SendError<ClientBroadcastMessage>) -> Self {
        HandlePublishError::BroadcastSend(error)
    }
}

impl From<MqttPacketEncoderError> for HandlePublishError {
    fn from(mqtt_packet_encoder_error: MqttPacketEncoderError) -> Self {
        HandlePublishError::Framed(mqtt_packet_encoder_error)
    }
}

impl From<ClusterStoreError> for HandlePublishError {
    fn from(cluster_store_error: ClusterStoreError) -> Self {
        HandlePublishError::ClusterStore(cluster_store_error)
    }
}

impl Display for HandlePublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for HandlePublishError {}

impl<S: ClusterStore> PublishHandler<S> {
    pub(crate) fn new(
        broker_store: S,
        clients_tx: broadcast::Sender<ClientBroadcastMessage>,
        max_topic_alias: u16,
    ) -> Self {
        PublishHandler {
            broker_store,
            clients_tx,
            max_topic_alias,
        }
    }

    pub(crate) async fn handle<Framed: MqttSinkStream>(
        &mut self,
        framed: &mut Framed,
        topic_alias_map: &mut HashMap<u16, String>,
        publish: Publish,
    ) -> Result<(), HandlePublishError> {
        // 3.3.4 PUBLISH Actions
        let topic_name = match publish.topic_alias {
            Some(topic_alias) if topic_alias == 0 || topic_alias > self.max_topic_alias => {
                framed
                    .send(MqttPacket::Disconnect(Disconnect {
                        disconnect_reason: DisconnectReason::TopicAliasInvalid,
                        server_reference: None,
                        session_expiry_interval: None,
                        user_properties: None,
                        reason_string: None,
                    }))
                    .await?;
                return Err(HandlePublishError::TopicAlias(
                    "'topic_alias' is or > MAX_TOPIC_ALIAS.".to_string(),
                ));
            }
            Some(topic_alias) if publish.topic_name.is_empty() => {
                if let Some(topic_name) = topic_alias_map.get(&topic_alias) {
                    topic_name
                } else {
                    framed
                        .send(MqttPacket::Disconnect(Disconnect {
                            disconnect_reason: DisconnectReason::ProtocolError,
                            server_reference: None,
                            session_expiry_interval: None,
                            user_properties: None,
                            reason_string: None,
                        }))
                        .await?;
                    return Err(HandlePublishError::TopicAlias(format!("Couldn't find the topic name from provided topic alias = {} and no topic name was provided.", topic_alias)));
                }
            }
            Some(topic_alias) => {
                topic_alias_map.insert(topic_alias, publish.topic_name.clone());
                &publish.topic_name
            }
            None if publish.topic_name.is_empty() => {
                framed
                    .send(MqttPacket::Disconnect(Disconnect {
                        disconnect_reason: DisconnectReason::ProtocolError,
                        server_reference: None,
                        session_expiry_interval: None,
                        user_properties: None,
                        reason_string: None,
                    }))
                    .await?;
                return Err(HandlePublishError::TopicAlias(
                    "Neither topic alias nor topic name was provided.".to_string(),
                ));
            }
            None => &publish.topic_name,
        };

        let retain = publish.retain;
        let payload_is_empty = publish.payload.is_empty();

        match &publish.qos {
            QoS::AtMostOnce => {
                broadcast_publish(
                    &self.clients_tx,
                    Publish {
                        topic_name: topic_name.to_string(),
                        ..publish.clone()
                    },
                );
            }
            QoS::AtLeastOnce => {
                let packet_identifier = if let Some(packet_identifier) = publish.packet_identifier {
                    packet_identifier
                } else {
                    todo!("error")
                };

                broadcast_publish(
                    &self.clients_tx,
                    Publish {
                        topic_name: topic_name.to_string(),
                        ..publish.clone()
                    },
                );

                framed
                    .send(MqttPacket::PubAck(PubAck {
                        packet_identifier,
                        reason: PubAckReason::Success,
                        reason_string: None,
                        user_properties: None,
                    }))
                    .await?;
            }
            QoS::ExactlyOnce => todo!("not impl exactly once in incoming PUBLISH handling."),
        }

        match (retain, payload_is_empty) {
            (true, true) => {
                self.broker_store.remove_retained(topic_name).await?;
            }
            (true, false) => {
                self.broker_store
                    .replace_retained(
                        topic_name,
                        Publish {
                            topic_name: topic_name.to_string(),
                            ..publish
                        },
                    )
                    .await?;
            }
            _ => {}
        }

        Ok(())
    }
}

fn broadcast_publish(broadcast_tx: &broadcast::Sender<ClientBroadcastMessage>, publish: Publish) {
    // TODO: store to some session and let another task handle the broadcast publish,
    // this will mean that a PubAck sent from the server actually means that the message will be processed
    match broadcast_tx.send(ClientBroadcastMessage::Publish {
        received_instant: Instant::now(),
        publish: Arc::new(publish),
    }) {
        Ok(estimated_receivers) => tracing::info!("An incoming PUBLISH message was sent to ~{} potential receivers.", estimated_receivers),
        Err(_) => unreachable!("Since the caller of this function also holds a receiving end of this channel this can never fail."),
    };
}
#[cfg(test)]
mod tests {
    use super::PublishHandler;
    use crate::broker_store::MemoryClusterStore;
    use crate::publish::HandlePublishError;
    use crate::ClientBroadcastMessage;
    use mqtt_protocol::codec::MqttPacketDecoder;
    use mqtt_protocol::{DisconnectReason, MqttPacket, Payload, PubAckReason, Publish, QoS};
    use std::collections::HashMap;
    use std::error::Error;
    use tokio::io::DuplexStream;
    use tokio::sync::broadcast;
    use tokio_stream::StreamExt;
    use tokio_util::codec::Framed;

    #[tokio::test]
    async fn publish_topic_alias_tests() -> Result<(), Box<dyn Error>> {
        let store = MemoryClusterStore::new();
        let (tx, mut rx) = broadcast::channel(10);
        let (mut client_side, mut server_side) = create_client();
        let mut handler = PublishHandler::new(store, tx, 1);

        let mut topic_alias_map = HashMap::new();
        let publish = Publish {
            topic_name: "foobar".to_string(),
            payload: Payload::Unspecified(vec![1, 2, 3, 4]),
            qos: QoS::AtMostOnce,
            retain: false,
            topic_alias: Some(1),
            subscription_identifier: None,
            content_type: None,
            correlation_data: None,
            duplicate: false,
            message_expiry_interval: None,
            response_topic: None,
            packet_identifier: Some(1),
            user_properties: None,
        };
        handler
            .handle(&mut server_side, &mut topic_alias_map, publish.clone())
            .await?;

        assert_eq!(Some("foobar"), topic_alias_map.get(&1).map(|v| v.as_str()));

        let ClientBroadcastMessage::Publish {
            publish: broadcasted_publish,
            ..
        } = rx.recv().await?;
        assert_eq!(publish.topic_name, broadcasted_publish.topic_name);

        let publish = Publish {
            topic_name: String::new(),
            payload: Payload::Unspecified(vec![1, 2, 3, 4]),
            qos: QoS::AtMostOnce,
            retain: false,
            topic_alias: Some(1),
            subscription_identifier: None,
            content_type: None,
            correlation_data: None,
            duplicate: false,
            message_expiry_interval: None,
            response_topic: None,
            packet_identifier: Some(2),
            user_properties: None,
        };
        handler
            .handle(&mut server_side, &mut topic_alias_map, publish.clone())
            .await?;

        let ClientBroadcastMessage::Publish {
            publish: broadcasted_publish,
            ..
        } = rx.recv().await?;
        assert_eq!("foobar", &broadcasted_publish.topic_name);

        let publish = Publish {
            topic_name: "rall".to_string(),
            payload: Payload::Unspecified(vec![1, 2, 3, 4]),
            qos: QoS::AtMostOnce,
            retain: false,
            topic_alias: Some(2),
            subscription_identifier: None,
            content_type: None,
            correlation_data: None,
            duplicate: false,
            message_expiry_interval: None,
            response_topic: None,
            packet_identifier: Some(3),
            user_properties: None,
        };

        let Err(HandlePublishError::TopicAlias(_)) = handler.handle(&mut server_side, &mut topic_alias_map, publish.clone()).await else {
            Err("max_topic_alias is 1 and the 2nd topic alias should therefore fail.")?
        };

        let Some(response) = client_side.next().await else {
            Err("client_side should not be closed.")?
        };

        let MqttPacket::Disconnect(disconnect) = response? else {
            Err("client_side should have received a PubAck.")?
        };

        assert_eq!(
            DisconnectReason::TopicAliasInvalid,
            disconnect.disconnect_reason
        );

        Ok(())
    }

    #[tokio::test]
    async fn publish_qos1_puback_test() -> Result<(), Box<dyn Error>> {
        let store = MemoryClusterStore::new();
        let (tx, mut rx) = broadcast::channel(10);
        let (mut client_side, mut server_side) = create_client();
        let mut handler = PublishHandler::new(store, tx, 1);

        let mut topic_alias_map = HashMap::new();
        let publish = Publish {
            topic_name: "foobar".to_string(),
            payload: Payload::Unspecified(vec![1, 2, 3, 4]),
            qos: QoS::AtLeastOnce,
            retain: false,
            topic_alias: None,
            subscription_identifier: None,
            content_type: None,
            correlation_data: None,
            duplicate: false,
            message_expiry_interval: None,
            response_topic: None,
            packet_identifier: Some(1),
            user_properties: None,
        };

        handler
            .handle(&mut server_side, &mut topic_alias_map, publish.clone())
            .await?;

        assert_eq!(0, topic_alias_map.len());

        let ClientBroadcastMessage::Publish {
            publish: broadcasted_publish,
            ..
        } = rx.recv().await?;
        assert_eq!(publish.topic_name, broadcasted_publish.topic_name);

        let Some(response) = client_side.next().await else {
            Err("client_side should not be closed.")?
        };

        let MqttPacket::PubAck(pub_ack) = response? else {
            Err("client_side should have received a PubAck.")?
        };

        assert_eq!(1, pub_ack.packet_identifier);
        assert_eq!(PubAckReason::Success, pub_ack.reason);

        Ok(())
    }

    fn create_client() -> (
        Framed<DuplexStream, MqttPacketDecoder>,
        Framed<DuplexStream, MqttPacketDecoder>,
    ) {
        let (client, server) = tokio::io::duplex(4096);
        (
            Framed::new(client, MqttPacketDecoder {}),
            Framed::new(server, MqttPacketDecoder {}),
        )
    }
}
