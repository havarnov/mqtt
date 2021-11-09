mod topic_filter;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_util::codec::Framed;

use crate::topic_filter::TopicFilter;
use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{
    ConnAck, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Publish, QoS, SubAck,
    SubscribeReason,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let broker = Arc::new(StandardBroker {
        clients: dashmap::DashMap::new(),
        retained: dashmap::DashMap::new(),
    });
    listener(broker.clone()).await
}

#[async_trait]
trait Broker {
    async fn incoming_connect(
        &self,
        client_identifier: &str,
        client_tx: UnboundedSender<ClientMessage>,
    );

    async fn new_subscription(
        &self,
        tx: UnboundedSender<ClientMessage>,
        _topic_filter: &TopicFilter,
    ) -> Result<(), Box<dyn Error>>;

    async fn publish(
        &self,
        publish: Publish,
    ) -> Result<(), Box<dyn Error>>;
}

struct StandardBroker {
    clients: dashmap::DashMap<String, UnboundedSender<ClientMessage>>,
    retained: dashmap::DashMap<String, Publish>,
}

#[async_trait]
impl Broker for StandardBroker {
    #[allow(clippy::async_yields_async)]
    async fn incoming_connect(
        &self,
        client_identifier: &str,
        client_tx: UnboundedSender<ClientMessage>,
    ) {
        if let Some(tx) = self
            .clients
            .insert(client_identifier.to_string(), client_tx)
        {
            let (disconnect_tx, disconnect_rx) = oneshot::channel();
            if tx
                .send(ClientMessage::SessionTakenOver(disconnect_tx))
                .is_err()
                || disconnect_rx.await.is_err()
            {
                // client has already disconnected.
                println!("TODO: client has disconnected, remove from connected clients.")
            }
        }
    }

    async fn new_subscription(
        &self,
        _client_tx: UnboundedSender<ClientMessage>,
        _topic_filter: &TopicFilter,
    ) -> Result<(), Box<dyn Error>> {

        for retained in self.retained.iter().filter(|r| _topic_filter.matches(&r.topic_name)) {
            _client_tx.send(ClientMessage::Publish(retained.clone()))?;
        }

        Ok(())
    }

    async fn publish(&self, publish: Publish) -> Result<(), Box<dyn Error>> {
        if publish.retain {
            if publish.payload.is_empty() {
                self.retained.remove(&publish.topic_name);
            } else {
                self.retained.insert(publish.topic_name.clone(), publish.clone());
            }
        }

        for client in self.clients.iter() {
            match client.send(ClientMessage::Publish(publish.clone())) {
                Ok(_) => (),
                Err(_) => (),
            }
        }

        Ok(())
    }
}

async fn listener<B: 'static + Broker + Send + Sync>(broker: Arc<B>) -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:6142";
    let listener = TcpListener::bind(&addr).await?;
    loop {
        let broker = broker.clone();
        // // Asynchronously wait for an inbound TcpStream.
        let (stream, addr) = listener.accept().await?;

        // Spawn our handler to be run asynchronously.
        tokio::spawn(async move {
            println!("accepted connection");
            if let Err(e) = process(broker, stream, addr).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

#[derive(Debug)]
enum ClientMessage {
    SessionTakenOver(oneshot::Sender<()>),
    Publish(Publish),
}

#[derive(Debug)]
struct ClientSubscription {
    topic_filter: TopicFilter,
    subscription_identifier: Option<u32>,
    maximum_qos: QoS,
    retain_as_published: bool,
}

async fn process<B: Broker>(
    broker: Arc<B>,
    stream: TcpStream,
    _addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut framed = Framed::new(stream, MqttPacketDecoder {});

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let _client_identifier = handle_connect(&mut framed, broker.deref(), tx.clone()).await?;

    let mut topic_alias_map = HashMap::new();
    let mut subscriptions = HashMap::new();

    loop {
        tokio::select! {
            packet = framed.next() => {
                match packet {
                    Some(Ok(MqttPacket::PingReq)) => {
                        println!("ping received.");
                        framed.send(MqttPacket::PingResp).await?;
                    }
                    Some(Ok(MqttPacket::Disconnect(_d))) => {
                        println!("client disconnected.");
                        return Ok(());
                    }
                    Some(Ok(MqttPacket::Publish(publish))) => {
                        println!("client published packet: {:?}.", publish);

                        // 3.3.4 PUBLISH Actions
                        let topic_name = match publish.topic_alias {
                            // TODO: configure max topic_alias
                            Some(topic_alias) if topic_alias == 0 => {
                                framed.send(MqttPacket::Disconnect(Disconnect {
                                    disconnect_reason: DisconnectReason::TopicAliasInvalid,
                                    server_reference: None,
                                    session_expiry_interval: None,
                                    user_properties: None,
                                    reason_string: None,
                                })).await?;
                                return Ok(());
                            },
                            Some(topic_alias) if publish.topic_name.is_empty() => {
                                if let Some(topic_name) = topic_alias_map.get(&topic_alias) {
                                    topic_name
                                } else {
                                    framed.send(MqttPacket::Disconnect(Disconnect {
                                        disconnect_reason: DisconnectReason::ProtocolError,
                                        server_reference: None,
                                        session_expiry_interval: None,
                                        user_properties: None,
                                        reason_string: None,
                                    })).await?;
                                    return Ok(());
                                }
                            },
                            Some(topic_alias) => {
                                topic_alias_map.insert(topic_alias, publish.topic_name.clone());
                                &publish.topic_name
                            },
                            None if publish.topic_name.is_empty() => {
                                framed.send(MqttPacket::Disconnect(Disconnect {
                                    disconnect_reason: DisconnectReason::ProtocolError,
                                    server_reference: None,
                                    session_expiry_interval: None,
                                    user_properties: None,
                                    reason_string: None,
                                })).await?;
                                return Ok(());
                            },
                            None => &publish.topic_name,
                        };

                        broker.publish(Publish { topic_name: topic_name.to_string(), ..publish.clone() }).await?;
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        // TODO: handle session.

                        println!("client subscribed to: {:?}", subscribe.topic_filters);

                        let mut reasons = Vec::new();
                        for topic_filter in subscribe.topic_filters.iter() {

                            // TODO: handle shared subscriptions.

                            let t = TopicFilter::new(&topic_filter.filter);

                            if let Err(_) = t {
                                // TODO: invalid topic filter
                                reasons.push(SubscribeReason::UnspecifiedError);
                                continue;
                            }

                            subscriptions.insert(
                                topic_filter.filter.clone(),
                                ClientSubscription {
                                    topic_filter: t.unwrap(),
                                    subscription_identifier: subscribe.subscription_identifier,
                                    maximum_qos: topic_filter.maximum_qos.clone(),
                                    retain_as_published: topic_filter.retain_as_published,
                                });

                            let reason = match broker.new_subscription(
                                    tx.clone(),
                                    &TopicFilter::new(&topic_filter.filter).unwrap()).await {
                                Ok(_) => SubscribeReason::GrantedQoS0,
                                Err(_) => SubscribeReason::UnspecifiedError,
                            };

                            reasons.push(reason);
                        }

                        framed
                            .send(MqttPacket::SubAck(SubAck {
                                packet_identifier: subscribe.packet_identifier,
                                reason_string: None,
                                user_properties: None,
                                reasons,
                            }))
                            .await?;
                    }
                    Some(Ok(_)) => {
                        unimplemented!("packet type not impl.")
                    }
                    Some(Err(e)) => {
                        println!("error: {}", e);
                        return Ok(());
                    },
                    None => {
                        println!("client disconnected without disconnect packet.");
                        return Ok(());
                    }
                }
            },
            client_message = rx.recv() => {
                match client_message {
                    Some(ClientMessage::SessionTakenOver(tx)) => {
                        framed.send(MqttPacket::Disconnect(Disconnect {
                            disconnect_reason: DisconnectReason::SessionTakenOver,
                            server_reference: None,
                            session_expiry_interval: None,
                            user_properties: None,
                            reason_string: None,
                        }))
                        .await?;
                        tx.send(()).map_err(|_| Box::new(ClientHandlerError::DisconnectError))?;
                        return Ok(())
                    },
                    Some(ClientMessage::Publish(publish)) => {
                        for subscription  in subscriptions.values() {

                            // TODO: handle retain_handling

                            if !subscription.topic_filter.matches(&publish.topic_name) {
                                continue;
                            }

                            let qos = if subscription.maximum_qos > publish.qos {
                                publish.qos.clone()
                            } else {
                                subscription.maximum_qos.clone()
                            };

                            let retain = if subscription.retain_as_published {
                                publish.retain
                            } else {
                                false
                            };

                            // 3.3.2.2 Packet Identifier
                            // The Packet Identifier field is only present in PUBLISH packets where the QoS level is 1 or 2.
                            let packet_identifier = if qos > QoS::AtMostOnce {
                                publish.packet_identifier
                            } else {
                                None
                            };

                            // 3.3.2.3.3 Message Expiry Interval
                            // The PUBLISH packet sent to a Client by the Server MUST contain a
                            // Message Expiry Interval set to the received value minus the time that
                            // the Application Message has been waiting in the Server
                            // TODO: find out the difference between the time the message was received and the time it was sent
                            let message_expiry_interval = publish.message_expiry_interval.map(|e| e - 0u32);

                            let p = Publish {
                                duplicate: false,
                                qos,
                                retain,
                                topic_name: publish.topic_name.clone(),
                                packet_identifier,
                                payload_format_indicator: publish.payload_format_indicator,
                                message_expiry_interval,
                                topic_alias: None,
                                response_topic: publish.response_topic.clone(),
                                correlation_data: publish.correlation_data.clone(),
                                user_properties: publish.user_properties.clone(),
                                // TODO: check how to handle topic names that matches multiple topic filters (for the same client)?
                                subscription_identifier: subscription.subscription_identifier,
                                content_type: publish.content_type.clone(),
                                payload: publish.payload.clone()
                            };

                            framed.send(MqttPacket::Publish(p)).await?;
                        }
                    },
                    None =>
                        // TODO: what does this mean? The broker has dropped the session taken over tx...
                        return Ok(())
                }
            },
        }
    }
}

#[derive(Debug, Clone)]
enum ClientHandlerError {
    ConnectError,
    DisconnectError,
}

impl std::fmt::Display for ClientHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "something happened during connection.")
    }
}

impl std::error::Error for ClientHandlerError {}

async fn handle_connect<B: Broker>(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    broker: &B,
    client_tx: UnboundedSender<ClientMessage>,
) -> Result<String, Box<dyn Error>> {
    // handle connection
    let connection_timeout = sleep(Duration::from_millis(20000));
    tokio::select! {
        packet = framed.next() =>
            match packet {
                Some(Ok(MqttPacket::Connect(c))) => {
                    println!("{:?}", c);

                    let assigned = if c.client_identifier.is_empty() {
                        // TODO: assign a server generated client identifier.
                        println!("client identifier is empty");
                        Some(format!("{}", rand::random::<u128>()))
                    } else {
                        None
                    };

                    if !c.clean_start {
                        framed.send(MqttPacket::ConnAck(ConnAck {
                            connect_reason: ConnectReason::ImplementationSpecificError,
                            reason_string: Some("Session not supported".to_string()),
                            session_present: false,
                            session_expiry_interval: None,
                            receive_maximum: None,
                            maximum_qos: None,
                            retain_available: None,
                            maximum_packet_size: None,
                            assigned_client_identifier: None,
                            topic_alias_maximum: None,
                            user_properties: None,
                        })).await?;

                        println!("non clean start not supported");
                        return Err(Box::new(ClientHandlerError::ConnectError));
                    }

                    broker.incoming_connect(assigned.as_ref().unwrap_or(&c.client_identifier), client_tx).await;

                    framed
                        .send(MqttPacket::ConnAck(ConnAck {
                            session_present: false,
                            connect_reason: ConnectReason::Success,
                            session_expiry_interval: None,
                            receive_maximum: None,
                            maximum_qos: Some(0u8),
                            retain_available: None,
                            maximum_packet_size: None,
                            assigned_client_identifier: assigned,
                            topic_alias_maximum: None,
                            reason_string: None,
                            user_properties: None,
                        }))
                        .await?;

                        Ok(c.client_identifier)
                }
                a => {
                    println!("Connection error: {:?}", a);
                    Err(Box::new(ClientHandlerError::ConnectError))
                },
        },
        _ = connection_timeout => {
            // connection_timeout completed before we got any 'Connection' packet. From the MQTT v5 specification:
            //   If the Server does not receive a CONNECT packet within a reasonable amount
            //   of time after the Network Connection is established, the Server SHOULD close the Network Connection.
            println!("connection timeout");
            Err(Box::new(ClientHandlerError::ConnectError))
        }
    }
}
