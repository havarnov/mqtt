mod session;
mod topic_filter;

use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::sleep;
use tokio_util::codec::Framed;

use crate::topic_filter::TopicFilter;
use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{
    ConnAck, Connect, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Publish, QoS,
    SubAck, Subscribe, SubscribeReason, UnsubAck, UnsubscribeReason,
};

// TODO: consts that should be configurable
const MAX_KEEP_ALIVE: u16 = 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    listener_v2().await
}

async fn listener_v2() -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:6142";
    let listener = TcpListener::bind(&addr).await?;
    let handler = Arc::new(IncomingConnectionHandler {
        clients: dashmap::DashMap::new(),
    });
    let (broadcast_tx, _) = broadcast::channel(1000);
    loop {
        // // Asynchronously wait for an inbound TcpStream.
        let (stream, _addr) = listener.accept().await?;
        let handler = handler.clone();
        let broadcast_tx = broadcast_tx.clone();

        // Spawn our handler to be run asynchronously.
        tokio::spawn(async move {
            println!("accepted connection");
            if let Err(e) = handler.incoming_connection(stream, broadcast_tx).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

#[derive(Debug)]
struct ClientSubscription {
    topic_filter: TopicFilter,
    subscription_identifier: Option<u32>,
    maximum_qos: QoS,
    retain_as_published: bool,
}

async fn process_v2(
    mut rx: UnboundedReceiver<ClientMessageV2>,
    broadcast_tx: broadcast::Sender<ClientBroadcastMessage>,
) -> Result<(), Box<dyn Error>> {
    let mut framed: Option<Framed<TcpStream, MqttPacketDecoder>> = None;
    let mut broadcast_rx = broadcast_tx.subscribe();

    let mut topic_alias_map = HashMap::new();
    let mut subscriptions: HashMap<String, ClientSubscription> = HashMap::new();

    let mut since_last: tokio::time::Instant = tokio::time::Instant::now();
    let mut keep_alive = 10;
    let mut keep_alive_timeout: tokio::time::Sleep;
    let mut keep_alive_timeout_duration = Duration::from_secs(keep_alive as u64);

    loop {
        keep_alive_timeout = sleep(keep_alive_timeout_duration);

        tokio::select! {
            _ = keep_alive_timeout => {
                // https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901045
                // 3.1.2.10 Keep Alive
                // If the Keep Alive value is non-zero and the Server does not receive an MQTT Control Packet
                // from the Client within one and a half times the Keep Alive time period, it MUST close the
                // Network Connection to the Client as if the network had failed.
                if since_last.elapsed().as_secs_f64() > (keep_alive as f64 * 1.5) {
                    todo!("if connection is active, fail. If not then we must keep going.");
                    // return Err(Box::new(ClientHandlerError::KeepAliveTimeout));
                }

                keep_alive_timeout_duration = match (keep_alive as f64 * 1.5) - since_last.elapsed().as_secs_f64() {
                    x if x < 0.0 => Duration::from_secs(0),
                    x => Duration::from_secs(x as u64),
                };
            },
            packet = async { framed.as_mut().expect("unreachable").next().await }, if framed.is_some() => {
                since_last = tokio::time::Instant::now();
                match packet {
                    Some(Ok(MqttPacket::Connect(_))) => {
                        todo!("What's the correct thing to do here, as we should never receive a second Connect packet.");
                    },
                    Some(Ok(MqttPacket::PingReq)) => {
                        println!("ping received.");
                        let framed = framed.as_mut().expect("must be some at this point");
                        framed.send(MqttPacket::PingResp).await?;
                    }
                    Some(Ok(MqttPacket::Disconnect(_d))) => {
                        println!("client disconnected.");
                        // 3.3.2.3.4 Topic Alias
                        // Topic Alias mappings exist only within a Network Connection and last only for the lifetime of that Network Connection.
                        topic_alias_map.clear();
                        // TODO: ???
                    }
                    Some(Ok(MqttPacket::Publish(publish))) => {
                        println!("client published packet: {:?}.", publish);
                        let framed = framed.as_mut().expect("must be some at this point");
                        handle_publish_v2(
                            framed,
                            &mut topic_alias_map,
                            publish,
                            &broadcast_tx)
                        .await?;
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        println!("client sent subscribe packet: {:?}.", subscribe);
                        let framed = framed.as_mut().expect("must be some at this point");
                        handle_subscribe_v2(
                            framed,
                            &mut subscriptions,
                            subscribe)
                        .await?;
                    }
                    Some(Ok(MqttPacket::Unsubscribe(unsubscribe))) => {
                        println!("client sent unsubscribe packet: {:?}.", unsubscribe);
                        let framed = framed.as_mut().expect("must be some at this point");

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
                    },
                    None => {
                        // TODO: handle will
                        println!("client disconnected without disconnect packet.");
                        framed = None;
                    }
                }
            },
            client_message = rx.recv() => {
                match client_message {
                    Some(ClientMessageV2::Connect { client_identifier, connect, framed: mut new_framed }) => {
                        // 3.3.2.3.4 Topic Alias
                        // Topic Alias mappings exist only within a Network Connection and last only for the lifetime of that Network Connection.
                        topic_alias_map.clear();

                        if let Some(mut framed) = framed {
                            framed.send(MqttPacket::Disconnect(Disconnect {
                                disconnect_reason: DisconnectReason::SessionTakenOver,
                                server_reference: None,
                                session_expiry_interval: None,
                                user_properties: None,
                                reason_string: None,
                            }))
                            .await?;
                            framed.close().await?;
                        }

                        let server_keep_alive = if connect.keep_alive == 0 || connect.keep_alive > MAX_KEEP_ALIVE {
                            Some(MAX_KEEP_ALIVE)
                        } else {
                            None
                        };

                        keep_alive = server_keep_alive.unwrap_or(connect.keep_alive);

                        if !connect.clean_start {

                            let session_present = false;
                            // TODO: get session
                            // let session = broker.get_session_storage().await.get(&client_identifier).await;
                            //
                            // if let Some(mut session) = session {
                            //     let session = match session.end_timestamp {
                            //         Some(end_timestamp) if end_timestamp >= SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() => Session::new(),
                            //         _ => {
                            //             session.end_timestamp = None;
                            //             session_present = true;
                            //             session
                            //         }
                            //     };
                            //
                            //     broker.get_session_storage().await.upsert(&client_identifier, session).await?;
                            // }

                            new_framed.send(MqttPacket::ConnAck(ConnAck {
                                connect_reason: ConnectReason::Success,
                                session_present,
                                session_expiry_interval: None,
                                receive_maximum: None,
                                maximum_qos: None,
                                retain_available: None,
                                maximum_packet_size: None,
                                assigned_client_identifier: None,
                                topic_alias_maximum: None,
                                user_properties: None,
                                reason_string: None,
                                wildcard_subscription_available: None,
                                subscription_identifiers_available: None,
                                shared_subscription_available: None,
                                server_keep_alive: None,
                                response_information: None,
                                server_reference: None,
                                authentication_method: None,
                                authentication_data: None,
                            })).await?;
                        }
                        else {
                            // TODO: delete session from storage

                            let assigned_client_identifier =
                            if let ClientIdentifier::ServerAssigned { client_identifier } = client_identifier {
                                Some(client_identifier)
                            } else {
                                None
                            };

                            new_framed
                                .send(MqttPacket::ConnAck(ConnAck {
                                    session_present: false,
                                    connect_reason: ConnectReason::Success,
                                    session_expiry_interval: None,
                                    receive_maximum: None,
                                    maximum_qos: Some(QoS::AtMostOnce),
                                    retain_available: None,
                                    maximum_packet_size: None,
                                    assigned_client_identifier,
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
                        }

                        // here we replace the framed tcp stream we're listening to.
                        framed = Some(new_framed);
                    },
                    _ => unimplemented!("unimplemented handling of ClientMessage"),
                }
            },
            client_broadcast_message = broadcast_rx.recv() => {
                match client_broadcast_message {
                    Ok(ClientBroadcastMessage::Publish(publish)) => {
                        if let Some(framed) = framed.as_mut() {
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

                                let retain = subscription.retain_as_published && publish.retain;

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
                        } else {
                            todo!("impl incoming subscription if no network connection with client.")
                        }
                    },
                    Err(_) => todo!("impl."),
                }
            },
        }
    }
}

#[derive(Clone, Debug)]
enum ClientBroadcastMessage {
    Publish(Arc<Publish>),
}

enum ClientMessageV2 {
    Connect {
        client_identifier: ClientIdentifier,
        connect: Connect,
        framed: Framed<TcpStream, MqttPacketDecoder>,
    },
}

struct IncomingConnectionHandler {
    clients: dashmap::DashMap<String, UnboundedSender<ClientMessageV2>>,
}

impl IncomingConnectionHandler {
    async fn incoming_connection(
        &self,
        stream: TcpStream,
        broadcast: broadcast::Sender<ClientBroadcastMessage>,
    ) -> Result<(), Box<dyn Error>> {
        let mut framed = Framed::new(stream, MqttPacketDecoder {});
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (client_identifier, connect) = handle_connect_v2(&mut framed).await?;
        dbg!(self.clients.len());
        self.clients
            .entry(client_identifier.get_client_identifier())
            .or_insert_with(|| {
                tokio::spawn(async move {
                    if let Err(e) = process_v2(rx, broadcast).await {
                        println!("an error occurred; error = {:?}", e);
                    }
                });
                tx
            })
            .send(ClientMessageV2::Connect {
                client_identifier,
                connect,
                framed,
            })
            .map_err(|e| e.to_string())
            // TODO: retry mechanism
            .expect("Couldn't send message to the client");
        Ok(())
    }
}

#[derive(Debug)]
enum ClientIdentifier {
    Normal { client_identifier: String },
    ServerAssigned { client_identifier: String },
}

impl ClientIdentifier {
    fn get_client_identifier(&self) -> String {
        match self {
            ClientIdentifier::ServerAssigned { client_identifier } => client_identifier.clone(),
            ClientIdentifier::Normal { client_identifier } => client_identifier.clone(),
        }
    }
}

async fn handle_connect_v2(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
) -> Result<(ClientIdentifier, Connect), Box<dyn Error>> {
    let connection_timeout = sleep(Duration::from_millis(20000));
    tokio::select! {
        packet = framed.next() =>
            match packet {
                Some(Ok(MqttPacket::Connect(connect))) => {
                    println!("{:?}", connect);

                    if connect.client_identifier.is_empty() {
                        println!("client identifier is empty");
                        Ok((ClientIdentifier::ServerAssigned { client_identifier: format!("{}", rand::random::<u128>()) }, connect))
                    } else {
                        Ok((ClientIdentifier::Normal { client_identifier: connect.client_identifier.clone() }, connect))
                    }
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

async fn handle_subscribe_v2(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    subscriptions: &mut HashMap<String, ClientSubscription>,
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

        reasons.push(SubscribeReason::GrantedQoS0);
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

async fn handle_publish_v2(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    topic_alias_map: &mut HashMap<u16, String>,
    publish: Publish,
    broadcast_tx: &broadcast::Sender<ClientBroadcastMessage>,
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

    broadcast_tx.send(ClientBroadcastMessage::Publish(Arc::new(Publish {
        topic_name: topic_name.to_string(),
        ..publish
    })))?;

    Ok(())
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
