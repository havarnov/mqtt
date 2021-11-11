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
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_util::codec::Framed;

use crate::topic_filter::TopicFilter;
use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{
    ConnAck, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Publish, QoS, SubAck,
    Subscribe, SubscribeReason, UnsubAck, UnsubscribeReason,
};

// TODO: consts that should be configurable
const MAX_KEEP_ALIVE: u16 = 60;

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

    async fn publish(&self, publish: Arc<Publish>) -> Result<(), Box<dyn Error>>;
}

struct StandardBroker {
    clients: dashmap::DashMap<String, UnboundedSender<ClientMessage>>,
    retained: dashmap::DashMap<String, Arc<Publish>>,
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
        for retained in self
            .retained
            .iter()
            .filter(|r| _topic_filter.matches(&r.topic_name))
        {
            _client_tx.send(ClientMessage::Publish(retained.clone()))?;
        }

        Ok(())
    }

    async fn publish(&self, publish: Arc<Publish>) -> Result<(), Box<dyn Error>> {
        if publish.retain {
            if publish.payload.is_empty() {
                self.retained.remove(&publish.topic_name);
            } else {
                self.retained
                    .insert(publish.topic_name.clone(), publish.clone());
            }
        }

        for client in self.clients.iter() {
            if client
                .send(ClientMessage::Publish(publish.clone()))
                .is_err()
            {
                println!("TODO: client is disconnected an can't process any messages.")
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
    Publish(Arc<Publish>),
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

    let connect_information = handle_connect(&mut framed, broker.deref(), tx.clone()).await?;

    let mut topic_alias_map = HashMap::new();
    let mut subscriptions = HashMap::new();

    let mut since_last: tokio::time::Instant = tokio::time::Instant::now();
    let mut keep_alive_timeout: tokio::time::Sleep;
    let mut keep_alive_timeout_duration: Duration = Duration::from_secs(connect_information.keep_alive as u64);

    loop {
        keep_alive_timeout = sleep(keep_alive_timeout_duration);

        tokio::select! {
            _ = keep_alive_timeout => {
                // https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901045
                // 3.1.2.10 Keep Alive
                // If the Keep Alive value is non-zero and the Server does not receive an MQTT Control Packet
                // from the Client within one and a half times the Keep Alive time period, it MUST close the
                // Network Connection to the Client as if the network had failed.
                if since_last.elapsed().as_secs_f64() > (connect_information.keep_alive as f64 * 1.5) {
                    return Err(Box::new(ClientHandlerError::KeepAliveTimeout));
                }

                keep_alive_timeout_duration = match (connect_information.keep_alive as f64 * 1.5) - since_last.elapsed().as_secs_f64() {
                    x if x < 0.0 => Duration::from_secs(0),
                    x => Duration::from_secs(x as u64),
                };
            },
            packet = framed.next() => {
                since_last = tokio::time::Instant::now();

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
                        handle_publish(
                            broker.deref(),
                            &mut framed,
                            &mut topic_alias_map,
                            publish)
                        .await?;
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        println!("client sent subscribe packet: {:?}.", subscribe);
                        handle_subscribe(
                            broker.deref(),
                            &mut framed,
                            &mut subscriptions,
                            tx.clone(),
                            subscribe)
                        .await?;
                    }
                    Some(Ok(MqttPacket::Unsubscribe(unsubscribe))) => {
                        println!("client sent unsubscribe packet: {:?}.", unsubscribe);

                        let mut reasons = Vec::new();
                        for unsubscribe_topic_filter in unsubscribe.topic_filters.iter() {
                            if subscriptions.remove(unsubscribe_topic_filter).is_none() {
                                reasons.push(UnsubscribeReason::NoSubscriptionExisted);
                            } else {
                                reasons.push(UnsubscribeReason::Success);
                            }
                        }

                        framed.send(MqttPacket::UnsubAck(UnsubAck {
                            packet_identifier: unsubscribe.packet_identifier,
                            reason_string: None,
                            user_properties: None,
                            reasons,
                        })).await?;
                    }
                    Some(Ok(_)) => {
                        unimplemented!("packet type not impl.")
                    }
                    Some(Err(e)) => {
                        println!("error: {}", e);
                        return Ok(());
                    },
                    None => {
                        // TODO: handle will
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
                            // let message_expiry_interval = publish.message_expiry_interval.map(|e| e - 0u32);

                            let p = Publish {
                                duplicate: false,
                                qos,
                                retain,
                                topic_name: publish.topic_name.clone(),
                                packet_identifier,
                                payload_format_indicator: publish.payload_format_indicator,
                                message_expiry_interval: publish.message_expiry_interval,
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

async fn handle_subscribe<B: Broker>(
    broker: &B,
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    subscriptions: &mut HashMap<String, ClientSubscription>,
    tx: UnboundedSender<ClientMessage>,
    subscribe: Subscribe,
) -> Result<(), Box<dyn Error>> {
    // TODO: handle session.

    println!("client subscribed to: {:?}", subscribe.topic_filters);

    let mut reasons = Vec::new();
    for topic_filter in subscribe.topic_filters.iter() {
        // TODO: handle shared subscriptions.

        let t = TopicFilter::new(&topic_filter.filter);

        if t.is_err() {
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
            },
        );

        let reason = match broker
            .new_subscription(tx.clone(), &TopicFilter::new(&topic_filter.filter).unwrap())
            .await
        {
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
    Ok(())
}

async fn handle_publish<B: Broker>(
    broker: &B,
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    topic_alias_map: &mut HashMap<u16, String>,
    publish: Publish,
) -> Result<(), Box<dyn Error>> {
    // 3.3.4 PUBLISH Actions
    let topic_name = match publish.topic_alias {
        // TODO: configure max topic_alias
        Some(topic_alias) if topic_alias == 0 => {
            framed
                .send(MqttPacket::Disconnect(Disconnect {
                    disconnect_reason: DisconnectReason::TopicAliasInvalid,
                    server_reference: None,
                    session_expiry_interval: None,
                    user_properties: None,
                    reason_string: None,
                }))
                .await?;
            return Ok(());
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
                return Ok(());
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
            return Ok(());
        }
        None => &publish.topic_name,
    };

    broker
        .publish(Arc::new(Publish {
            topic_name: topic_name.to_string(),
            ..publish
        }))
        .await
}

#[derive(Debug, Clone)]
enum ClientHandlerError {
    ConnectError,
    DisconnectError,
    KeepAliveTimeout,
}

impl std::fmt::Display for ClientHandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "something happened during connection.")
    }
}

impl std::error::Error for ClientHandlerError {}

struct ConnectInformation {
    _client_identifier: String,
    keep_alive: u16,
}

async fn handle_connect<B: Broker>(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    broker: &B,
    client_tx: UnboundedSender<ClientMessage>,
) -> Result<ConnectInformation, Box<dyn Error>> {
    // handle connection
    let connection_timeout = sleep(Duration::from_millis(20000));
    tokio::select! {
        packet = framed.next() =>
            match packet {
                Some(Ok(MqttPacket::Connect(connect))) => {
                    println!("{:?}", connect);

                    let server_keep_alive = if connect.keep_alive == 0 || connect.keep_alive > MAX_KEEP_ALIVE {
                        Some(MAX_KEEP_ALIVE)
                    } else {
                        None
                    };

                    let assigned = if connect.client_identifier.is_empty() {
                        // TODO: assign a server generated client identifier.
                        println!("client identifier is empty");
                        Some(format!("{}", rand::random::<u128>()))
                    } else {
                        None
                    };

                    if !connect.clean_start {
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
                            wildcard_subscription_available: None,
                            subscription_identifiers_available: None,
                            shared_subscription_available: None,
                            server_keep_alive: None,
                            response_information: None,
                            server_reference: None,
                            authentication_method: None,
                            authentication_data: None,
                        })).await?;

                        println!("non clean start not supported");
                        return Err(Box::new(ClientHandlerError::ConnectError));
                    }

                    broker.incoming_connect(assigned.as_ref().unwrap_or(&connect.client_identifier), client_tx).await;

                    framed
                        .send(MqttPacket::ConnAck(ConnAck {
                            session_present: false,
                            connect_reason: ConnectReason::Success,
                            session_expiry_interval: None,
                            receive_maximum: None,
                            maximum_qos: Some(QoS::AtMostOnce),
                            retain_available: None,
                            maximum_packet_size: None,
                            assigned_client_identifier: assigned.clone(),
                            topic_alias_maximum: None,
                            reason_string: None,
                            user_properties: None,
                            wildcard_subscription_available: None,
                            subscription_identifiers_available: None,
                            shared_subscription_available: None,
                            server_keep_alive,
                            response_information: None,
                            server_reference: None,
                            authentication_method: None,
                            authentication_data: None,
                        }))
                        .await?;

                        Ok(ConnectInformation {
                            _client_identifier: assigned.unwrap_or(connect.client_identifier),
                            keep_alive: server_keep_alive.unwrap_or(connect.keep_alive),
                        })
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
