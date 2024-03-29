mod broker_store;
mod publish;
mod session;
mod topic_filter;

#[cfg(test)]
mod tests;

use dashmap::mapref::entry::Entry;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::{sleep, Instant};
use tokio_util::codec::Framed;
use tracing::trace;

use crate::broker_store::{ClusterStore, ClusterStoreError, MemoryClusterStore};
use crate::publish::{HandlePublishError, PublishHandler};
use crate::session::{
    AddSubscriptionResult, ClientSubscription, MemorySessionProvider, SessionError, SessionProvider,
};
use crate::topic_filter::TopicFilter;
use mqtt_protocol::codec::{MqttPacketDecoder, MqttPacketDecoderError, MqttPacketEncoderError};
use mqtt_protocol::{
    ConnAck, Connect, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Publish, QoS,
    RetainHandling, SubAck, Subscribe, SubscribeReason, UnsubAck, UnsubscribeReason, Will,
};

// TODO: consts that should be configurable
const MAX_KEEP_ALIVE: u16 = 60;
const MAX_CONNECT_DELAY: Duration = Duration::from_millis(20000);
const MAX_TOPIC_ALIAS: u16 = 10;

trait MqttSinkStream:
    SinkExt<MqttPacket, Error = MqttPacketEncoderError>
    + StreamExt<Item = Result<MqttPacket, MqttPacketDecoderError>>
    + Unpin
    + Debug
    + Send
{
}

impl<T> MqttSinkStream for T where
    T: SinkExt<MqttPacket, Error = MqttPacketEncoderError>
        + StreamExt<Item = Result<MqttPacket, MqttPacketDecoderError>>
        + Unpin
        + Debug
        + Send
{
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    console_subscriber::init();

    let session_provider = MemorySessionProvider::new();
    let cluster_store = MemoryClusterStore::new();
    listener(session_provider, cluster_store).await
}

async fn listener<P: 'static + SessionProvider, Store: 'static + ClusterStore>(
    session_provider: P,
    cluster_store: Store,
) -> Result<(), Box<dyn Error>> {
    let addr = "0.0.0.0:6142";
    let listener = TcpListener::bind(&addr).await?;
    let handler = Arc::new(IncomingConnectionHandler {
        clients: dashmap::DashMap::new(),
        session_provider,
        cluster_store,
    });
    let (broadcast_tx, _) = broadcast::channel(1000);
    loop {
        let (stream, _addr) = listener.accept().await?;

        let handler = handler.clone();
        let broadcast_tx = broadcast_tx.clone();
        let framed = Framed::new(stream, MqttPacketDecoder {});

        // Spawn our handler to be run asynchronously.
        tokio::spawn(async move {
            println!("accepted connection");
            if let Err(e) = handler.incoming_connection(framed, broadcast_tx).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

async fn process<Session: session::Session, Store: ClusterStore, T: MqttSinkStream>(
    mut connect_rx: UnboundedReceiver<ConnectMessage<T>>,
    broadcast_tx: broadcast::Sender<ClientBroadcastMessage>,
    mut session: Session,
    mut cluster_store: Store,
) -> Result<(), Box<dyn Error>>
where
    <T as futures::Sink<MqttPacket>>::Error: std::error::Error,
{
    let mut publish_handler =
        PublishHandler::new(cluster_store.clone(), broadcast_tx.clone(), MAX_TOPIC_ALIAS);

    let mut framed: Option<T> = None;
    let mut broadcast_rx = broadcast_tx.subscribe();

    let mut topic_alias_map = HashMap::new();

    let mut will: Option<Will> = None;
    let will_expiry_timeout = sleep(Duration::ZERO);
    // needed to be able to mutate without consuming.
    // see https://docs.rs/tokio/latest/tokio/time/struct.Sleep.html
    tokio::pin!(will_expiry_timeout);

    let mut since_last: tokio::time::Instant = tokio::time::Instant::now();
    let mut keep_alive = Duration::from_secs(0);
    let keep_alive_timeout = sleep(Duration::ZERO);
    // needed to be able to mutate without consuming.
    // see https://docs.rs/tokio/latest/tokio/time/struct.Sleep.html
    tokio::pin!(keep_alive_timeout);

    loop {
        tokio::select! {
            _ = &mut will_expiry_timeout, if framed.is_none() && will.is_some() => {
                let will = will.take().expect("must be Some at this point.");

                broadcast_tx.send(ClientBroadcastMessage::Publish {
                        received_instant: Instant::now(),
                        publish: Arc::new(Publish {
                            duplicate: false,
                            qos: will.qos,
                            retain: will.retain,
                            topic_name: will.topic,
                            packet_identifier: None,
                            message_expiry_interval: will.message_expiry_interval,
                            topic_alias: None,
                            response_topic: will.response_topic,
                            correlation_data: will.correlation_data,
                            user_properties: will.user_properties,
                            subscription_identifier: None,
                            content_type: will.content_type,
                            payload: will.payload,
                    }),
                })?;
            },
            _ = &mut keep_alive_timeout, if framed.is_some() => {
                // https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901045
                // 3.1.2.10 Keep Alive
                // If the Keep Alive value is non-zero and the Server does not receive an MQTT Control Packet
                // from the Client within one and a half times the Keep Alive time period, it MUST close the
                // Network Connection to the Client as if the network had failed.
                if since_last.elapsed().as_secs_f64() > (keep_alive.as_secs_f64() * 1.5) {
                    // Setting framed to None aka dropping the network connection
                    framed = None;
                } else {
                    match (keep_alive.as_secs_f64() * 1.5) - since_last.elapsed().as_secs_f64() {
                        delta if delta < 0.0 => keep_alive_timeout.as_mut().reset(tokio::time::Instant::now()),
                        delta => keep_alive_timeout.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs_f64(delta))
                    };
                }

            },
            packet = async { framed.as_mut().expect("unreachable").next().await }, if framed.is_some() => {
                since_last = tokio::time::Instant::now();
                match packet {
                    Some(Ok(MqttPacket::Connect(_))) => {
                        // A Connect packet can only come to the processor through the ConnectMessage
                        // and if we ever end up here it means the client has sent multiple Connect packets.
                        // Therefore we're closing the network connection.
                        // From the spec:
                        //
                        // 3.1 CONNECT – Connection Request
                        // (...)
                        // A Client can only send the CONNECT packet once over a Network Connection.
                        // The Server MUST process a second CONNECT packet sent from a Client as a Protocol Error and close the Network Connection [MQTT-3.1.0-2].
                        // (...)
                        framed = None;
                    },
                    Some(Ok(MqttPacket::PingReq)) => {
                        println!("ping received.");
                        match framed.as_mut().expect("must be some at this point").send(MqttPacket::PingResp).await {
                            Err(MqttPacketEncoderError::IOError(io_error)) => {
                                // close the network connection on any io error.
                                println!("{:?}", io_error);
                                framed = None;
                            },
                            Err(_) => {}
                            _ => {}
                        }
                    }
                    Some(Ok(MqttPacket::Disconnect(disconnect))) => {
                        println!("client disconnected.");
                        // 3.3.2.3.4 Topic Alias
                        // (...)
                        // Topic Alias mappings exist only within a Network Connection and last only
                        // for the lifetime of that Network Connection.
                        // (...)
                        topic_alias_map.clear();

                        // 3.14.4 DISCONNECT Actions
                        // (...)
                        // On receipt of DISCONNECT with a Reason Code of 0x00 (Success) the Server:
                        //   MUST discard any Will Message associated with the current Connection without
                        //   publishing it [MQTT-3.14.4-3], as described in section 3.1.2.5.
                        // (...)
                        // 3.1.2.5 Will Flag
                        // (...)
                        // The Will Message MUST be removed from the stored Session State in the Server
                        // once it has been published or the Server has received a DISCONNECT packet
                        // with a Reason Code of 0x00 (Normal disconnection) from the Client [MQTT-3.1.2-10].
                        // (...)
                        // TODO: is it more correct with ```!= DisconnectReason::DisconnectWithWillMessage```?
                        if disconnect.disconnect_reason == DisconnectReason::NormalDisconnection {
                            will = None;
                        }

                        // Closing the network connection with the client.
                        framed = None;
                    }
                    Some(Ok(MqttPacket::Publish(publish))) => {
                        println!("client published packet: {:?}.", publish);
                        match publish_handler.handle(
                            framed.as_mut().expect("must be some at this point"),
                            &mut topic_alias_map,
                            publish,)
                        .await {
                            Ok(()) => (),
                            Err(HandlePublishError::TopicAlias(error)) => {
                                trace!("{}", error);
                                framed = None;
                            }
                            Err(HandlePublishError::Framed(error)) => {
                                println!("{:?}", error);
                                framed = None;
                            }
                            Err(HandlePublishError::BroadcastSend(error)) => return Err(Box::new(error)),
                            Err(HandlePublishError::ClusterStore(error)) => return Err(Box::new(error)),
                        }
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        println!("client sent subscribe packet: {:?}.", subscribe);
                        match handle_subscribe(
                            framed.as_mut().expect("must be some at this point"),
                            &mut session,
                            &mut cluster_store,
                            subscribe)
                        .await {
                            Ok(_) => {},
                            Err(HandleSubscribeError::FramedError(error)) => {
                                println!("{:?}", error);
                                framed = None;
                            }
                            Err(HandleSubscribeError::SessionError(_error)) => todo!("What to do with session errors??"),
                            Err(HandleSubscribeError::ClusterStore(_error)) => todo!("What to do with cluster store errors??"),
                        }
                    }
                    Some(Ok(MqttPacket::Unsubscribe(unsubscribe))) => {
                        println!("client sent unsubscribe packet: {:?}.", unsubscribe);
                        let framed = framed.as_mut().expect("must be some at this point");

                        let mut reasons = Vec::new();
                        for unsubscribe_topic_filter in unsubscribe.topic_filters {
                            reasons.push(match session.remove_subscription(unsubscribe_topic_filter.to_owned()).await {
                                Ok(Some(_)) => UnsubscribeReason::Success,
                                Ok(None) => UnsubscribeReason::NoSubscriptionExisted,
                                Err(_) => UnsubscribeReason::UnspecifiedError,
                            });
                        }

                        framed.send(MqttPacket::UnsubAck(UnsubAck {
                            packet_identifier: unsubscribe.packet_identifier,
                            reason_string: None,
                            user_properties: None,
                            reasons,
                        })).await?;
                    }
                    Some(Ok(MqttPacket::PubAck(puback))) => {
                        println!("client sent a puback packet: {:?}.", puback);
                        todo!("handle puback packet from client.");
                    }
                    Some(Ok(_)) => {
                        unimplemented!("packet type not impl.")
                    }
                    Some(Err(e)) => {
                        println!("error: {}", e);
                    },
                    None => {
                        println!("client disconnected without disconnect packet.");

                        // will handling
                        if let Some(will) = will.as_ref() {
                            will_expiry_timeout
                                .as_mut()
                                .reset(tokio::time::Instant::now() + Duration::from_secs(will.message_expiry_interval.unwrap_or(0) as u64));
                        }

                        framed = None;
                    }
                }
            },
            connect_message = connect_rx.recv() => {
                match connect_message {
                    Some(ConnectMessage { client_identifier, connect, framed: mut new_framed }) => {
                        since_last = tokio::time::Instant::now();

                        // Will handling
                        will = connect.will;

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

                        keep_alive = Duration::from_secs(server_keep_alive.unwrap_or(connect.keep_alive) as u64);

                        if !connect.clean_start {

                            let session_present = false;
                            session.set_endtimestamp(None).await?;

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
                                server_keep_alive,
                                response_information: None,
                                server_reference: None,
                                authentication_method: None,
                                authentication_data: None,
                            })).await?;
                        }
                        else {
                            // TODO: handle error
                            session.clear().await?;

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
                    None => unreachable!("The connect_rx can only be closed from this end, the sender in IncomingConnectionHandler will only be dropped if this process has already been drop."),
                }
            },
            client_broadcast_message = broadcast_rx.recv() => {
                match client_broadcast_message {
                    // https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901120
                    // If the Client specified a Subscription Identifier for any of the overlapping
                    // subscriptions the Server MUST send those Subscription Identifiers in the message
                    // which is published as the result of the subscriptions [MQTT-3.3.4-3].
                    // If the Server sends a single copy of the message it MUST include in the PUBLISH
                    // packet the Subscription Identifiers for all matching subscriptions which have
                    // a Subscription Identifiers, their order is not significant [MQTT-3.3.4-4].
                    // If the Server sends multiple PUBLISH packets it MUST send, in each of them,
                    // the Subscription Identifier of the matching subscription if it has a
                    // Subscription Identifier [MQTT-3.3.4-5].
                    Ok(ClientBroadcastMessage::Publish { received_instant, publish }) => {
                        let subscriptions = match session.get_subscriptions().await {
                            Ok(subscriptions) => subscriptions,
                            Err(_) => todo!("handle session error")
                        };

                        let publish_messages_to_send =
                            subscriptions
                            .iter()
                            .filter_map(|subscription| {

                                // TODO: handle retain_handling

                                if !subscription.topic_filter.matches(&publish.topic_name) {
                                    return None;
                                }

                                let qos = if subscription.maximum_qos > publish.qos {
                                    publish.qos
                                } else {
                                    subscription.maximum_qos
                                };

                                // 3.3.1.3 RETAIN
                                // The setting of the RETAIN flag in an Application Message forwarded by the Server
                                // from an established connection is controlled by the Retain As Published subscription option.
                                //
                                // - If the value of Retain As Published subscription option is set to 0,
                                //   the Server MUST set the RETAIN flag to 0 when forwarding an Application Message
                                //   regardless of how the RETAIN flag was set in the received PUBLISH packet [MQTT-3.3.1-12].
                                //
                                // - If the value of Retain As Published subscription option is set to 1,
                                //   the Server MUST set the RETAIN flag equal to the RETAIN flag in the received PUBLISH packet [MQTT-3.3.1-13].
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
                                let message_expiry_interval = publish.message_expiry_interval.map(|e| e - (Instant::now() - received_instant).as_secs() as u32);

                                // TODO: handle AtLeastOnce and ExactlyOnce

                                let publish = Publish {
                                    duplicate: false,
                                    qos,
                                    retain,
                                    topic_name: publish.topic_name.clone(),
                                    packet_identifier,
                                    message_expiry_interval,
                                    topic_alias: None,
                                    response_topic: publish.response_topic.clone(),
                                    correlation_data: publish.correlation_data.clone(),
                                    user_properties: publish.user_properties.clone(),
                                    subscription_identifier: subscription.subscription_identifier,
                                    content_type: publish.content_type.clone(),
                                    payload: publish.payload.clone()
                                };

                                Some(publish)
                            })
                            .collect::<Vec<_>>();

                        for publish in publish_messages_to_send {

                            let packet_identifier = publish.packet_identifier;

                            if framed.is_none() {
                                if publish.qos == QoS::AtLeastOnce {
                                    match session.add_unsent_atleastonce(packet_identifier.unwrap(), publish.clone()).await {
                                        Ok(_) => (),
                                        Err(_) => todo!("Handle session store error."),
                                    }
                                    // TODO: handle max messages in process?
                                }

                                // TODO: ???
                            } else {
                                let qos = publish.qos;
                                match framed.as_mut().expect("must be some at this point.").send(MqttPacket::Publish(publish)).await {
                                    Ok(()) => {
                                        if qos != QoS::AtMostOnce {
                                            match session.unacked_atleastonce(packet_identifier.unwrap()).await {
                                                Ok(_) => (),
                                                Err(_) => todo!("Handle session store error."),
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        framed = None;
                                        // TODO: sending of message failed, should the message be kept in the session state?
                                    }
                                }
                            }
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
    Publish {
        received_instant: Instant,
        publish: Arc<Publish>,
    },
}

#[derive(Debug)]
struct ConnectMessage<T: MqttSinkStream> {
    client_identifier: ClientIdentifier,
    connect: Connect,
    framed: T,
}

struct IncomingConnectionHandler<P: SessionProvider, T: MqttSinkStream, Store: ClusterStore> {
    clients: dashmap::DashMap<String, UnboundedSender<ConnectMessage<T>>>,
    session_provider: P,
    cluster_store: Store,
}

impl<P: 'static + SessionProvider, T: 'static + MqttSinkStream, Store: 'static + ClusterStore>
    IncomingConnectionHandler<P, T, Store>
{
    async fn incoming_connection(
        &self,
        mut framed: T,
        broadcast: broadcast::Sender<ClientBroadcastMessage>,
    ) -> Result<(), Box<dyn Error>> {
        let (client_identifier, connect) = handle_connect(&mut framed).await?;

        match self
            .clients
            .entry(client_identifier.get_client_identifier())
        {
            Entry::Occupied(occupied) => {
                if occupied.get().is_closed() {
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                    let session = self
                        .session_provider
                        .get(&client_identifier.get_client_identifier())
                        .await?;
                    let cluster_store = self.cluster_store.clone();
                    tokio::spawn(async move {
                        if let Err(e) = process(rx, broadcast, session, cluster_store).await {
                            println!("an error occurred; error = {:?}", e);
                        }
                    });

                    tx.send(ConnectMessage {
                        client_identifier,
                        connect,
                        framed,
                    })?;

                    occupied.replace_entry(tx);
                } else {
                    occupied.get().send(ConnectMessage {
                        client_identifier,
                        connect,
                        framed,
                    })?;
                }
            }
            Entry::Vacant(vacant) => {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                let session = self
                    .session_provider
                    .get(&client_identifier.get_client_identifier())
                    .await?;

                let cluster_store = self.cluster_store.clone();

                tokio::spawn(async move {
                    if let Err(e) = process(rx, broadcast, session, cluster_store).await {
                        println!("an error occurred; error = {:?}", e);
                    }
                });

                tx.send(ConnectMessage {
                    client_identifier,
                    connect,
                    framed,
                })?;

                vacant.insert(tx);
            }
        }

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

#[derive(Debug, Clone)]
enum HandleConnectError {
    ConnectNotFirstPacket,
    ConnectTimeoutError,
}

impl std::fmt::Display for HandleConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "something happened during connection.")
    }
}

impl std::error::Error for HandleConnectError {}

async fn handle_connect<T: MqttSinkStream>(
    framed: &mut T,
    // framed: &mut Framed<TcpStream, MqttPacketDecoder>,
) -> Result<(ClientIdentifier, Connect), HandleConnectError> {
    let connection_timeout = sleep(MAX_CONNECT_DELAY);
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
                    Err(HandleConnectError::ConnectNotFirstPacket)
                },
        },
        _ = connection_timeout => {
            // connection_timeout completed before we got any 'Connection' packet. From the MQTT v5 specification:
            //   If the Server does not receive a CONNECT packet within a reasonable amount
            //   of time after the Network Connection is established, the Server SHOULD close the Network Connection.
            println!("connection timeout");
            Err(HandleConnectError::ConnectTimeoutError)
        }
    }
}

#[derive(Debug)]
enum HandleSubscribeError {
    SessionError(SessionError),
    FramedError(MqttPacketEncoderError),
    ClusterStore(ClusterStoreError),
}

impl From<SessionError> for HandleSubscribeError {
    fn from(session_error: SessionError) -> Self {
        HandleSubscribeError::SessionError(session_error)
    }
}

impl From<MqttPacketEncoderError> for HandleSubscribeError {
    fn from(mqtt_packet_encoder_error: MqttPacketEncoderError) -> Self {
        HandleSubscribeError::FramedError(mqtt_packet_encoder_error)
    }
}

impl From<ClusterStoreError> for HandleSubscribeError {
    fn from(error: ClusterStoreError) -> Self {
        HandleSubscribeError::ClusterStore(error)
    }
}

impl std::fmt::Display for HandleSubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for HandleSubscribeError {}

async fn handle_subscribe<Session: session::Session, T: MqttSinkStream, Store: ClusterStore>(
    framed: &mut T,
    session: &mut Session,
    cluster_store: &mut Store,
    subscribe: Subscribe,
) -> Result<(), HandleSubscribeError> {
    println!("client subscribed to: {:?}", subscribe.topic_filters);

    let mut reasons = Vec::with_capacity(subscribe.topic_filters.len());
    for topic_filter_from_subscription in subscribe.topic_filters.iter() {
        if topic_filter_from_subscription.filter.starts_with("$shared") {
            todo!("handle shared subscriptions.");
        }

        let Ok(topic_filter) = TopicFilter::new(&topic_filter_from_subscription.filter) else {
            reasons.push(SubscribeReason::UnspecifiedError);
            continue;
        };

        /*
        If a Server receives a SUBSCRIBE packet containing a Topic Filter that is identical to a Non‑shared Subscription’s Topic Filter for the current Session,
        then it MUST replace that existing Subscription with a new Subscription [MQTT-3.8.4-3]. The Topic Filter in the new Subscription will be identical to that
        in the previous Subscription, although its Subscription Options could be different. If the Retain Handling option is 0, any existing retained messages
        matching the Topic Filter MUST be re-sent, but Applicaton Messages MUST NOT be lost due to replacing the Subscription [MQTT-3.8.4-4].
        */
        let add_subscription_result = session
            .add_subscription(
                topic_filter_from_subscription.filter.clone(),
                ClientSubscription {
                    topic_filter: topic_filter.clone(),
                    subscription_identifier: subscribe.subscription_identifier,
                    maximum_qos: topic_filter_from_subscription.maximum_qos,
                    retain_as_published: topic_filter_from_subscription.retain_as_published,
                },
            )
            .await?;

        // TODO: move this after SubAck.
        // Not necessary according to standard, but make sens impl wise.
        match (
            add_subscription_result,
            &topic_filter_from_subscription.retain_handling,
        ) {
            (_, &RetainHandling::SendRetained)
            | (AddSubscriptionResult::New, &RetainHandling::SendRetainedForNewSubscription) => {
                for retained in cluster_store.get_retained(&topic_filter).await? {
                    let retained = Publish {
                        retain: true,
                        ..retained
                    };
                    framed.send(MqttPacket::Publish(retained)).await?;
                }
            }
            _ => {}
        };

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
