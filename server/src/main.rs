mod topic_filter;

use async_trait::async_trait;
use futures::SinkExt;
use std::error::Error;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::topic_filter::TopicFilter;
use crate::SubscriptionMessage::{IncomingMessage, NewSubscriber};
use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{ConnAck, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Publish, SubAck, Subscribe, SubscribeReason};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let broker = Arc::new(StandardBroker {
        clients: dashmap::DashMap::new(),
        subscription_handlers: dashmap::DashMap::new(),
    });
    listener(broker.clone()).await
}


#[derive(Debug)]
enum SubscriptionMessage {
    NewSubscriber { tx: UnboundedSender<ClientMessage>, msg: Subscribe },
    IncomingMessage { msg: Publish },
}

async fn subscription_handler(
    topic_filter: TopicFilter,
    mut rx: UnboundedReceiver<SubscriptionMessage>,
) -> Result<(), Box<dyn Error>> {
    let mut txs = Vec::new();
    loop {
        tokio::select! {
            msg = rx.recv() => {
                use SubscriptionMessage::*;
                match msg {
                    Some(NewSubscriber { tx, msg }) => {
                        txs.push((tx, msg));
                        println!("newsub");
                    },
                    Some(IncomingMessage { msg }) => {
                        if topic_filter.matches(&msg.topic_name) {
                            for (tx, subscribe) in txs.iter() {
                                let msg = Publish {
                                    subscription_identifier: subscribe.subscription_identifier.clone(),
                                    ..msg.clone()
                                };
                                tx.send(ClientMessage::NewMessageOnSubscription(msg))?;
                            }
                            println!("incoming");
                        }
                    },
                    None => println!("none"),
                }
            }
        }
    }
}

#[async_trait]
trait Broker {
    async fn incoming_connect(
        &self,
        client_identifier: &str,
        client_tx: UnboundedSender<ClientMessage>,
    );

    async fn subscription_message(
        &self,
        subscription_identifier: &str,
        msg: SubscriptionMessage,
    ) -> Result<(), Box<dyn Error>>;
}

struct StandardBroker {
    clients: dashmap::DashMap<String, UnboundedSender<ClientMessage>>,
    subscription_handlers: dashmap::DashMap<String, UnboundedSender<SubscriptionMessage>>,
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
                println!("TODO: log error")
            }
        }
    }

    async fn subscription_message(
        &self,
        subscription_identifier: &str,
        msg: SubscriptionMessage,
    ) -> Result<(), Box<dyn Error>> {
        let topic_filter = TopicFilter::new(subscription_identifier)?;
        let tx = self
            .subscription_handlers
            .entry(subscription_identifier.to_string())
            .or_insert_with(|| {
                // TODO: spawn shared subscription handler if $shared.
                let (tx, rx) = unbounded_channel();
                tokio::spawn(async move {
                    if let Err(e) = subscription_handler(topic_filter, rx).await {
                        println!("{}", e);
                    }
                });
                tx
            });

        tx.send(msg)?;
        Ok(())
    }
}

async fn listener<B: 'static + Broker + Send + Sync>(broker: Arc<B>) -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:6142";
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
    NewMessageOnSubscription(Publish),
}

/// Process an individual chat client
async fn process<B: Broker>(
    broker: Arc<B>,
    stream: TcpStream,
    _addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut framed = Framed::new(stream, MqttPacketDecoder {});

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    handle_connect(&mut framed, broker.deref(), tx.clone()).await?;

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
                        broker.subscription_message(
                            &publish.topic_name,
                            IncomingMessage { msg: publish.clone() })
                        .await?;
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        println!("client subscribed to: {:?}", subscribe.topic_filters);

                        for topic_filter in subscribe.topic_filters.iter() {
                            broker.subscription_message(
                                &topic_filter.filter,
                                NewSubscriber { tx: tx.clone(), msg: subscribe.clone() }).await?;
                        }

                        framed
                            .send(MqttPacket::SubAck(SubAck {
                                packet_identifier: subscribe.packet_identifier,
                                reason_string: None,
                                user_properties: None,
                                reasons: vec![SubscribeReason::GrantedQoS0],
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
                    None => return Ok(())
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
                    Some(ClientMessage::NewMessageOnSubscription(publish)) => {
                        framed.send(MqttPacket::Publish(publish)).await?;
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
) -> Result<(), Box<dyn Error>> {
    // handle connection
    let connection_timeout = sleep(Duration::from_millis(1000));
    tokio::select! {
        packet = framed.next() =>
            match packet {
                Some(Ok(MqttPacket::Connect(c))) => {
                    println!("{:?}", c);
                    framed
                        .send(MqttPacket::ConnAck(ConnAck {
                            session_present: false,
                            connect_reason: ConnectReason::Success,
                            session_expiry_interval: None,
                            receive_maximum: None,
                            maximum_qos: None,
                            retain_available: None,
                            maximum_packet_size: None,
                            assigned_client_identifier: None,
                            topic_alias_maximum: None,
                            reason_string: None,
                            user_properties: None,
                        }))
                        .await?;

                        broker.incoming_connect(&c.client_identifier, client_tx).await;

                        Ok(())
                }
                _ => Err(Box::new(ClientHandlerError::ConnectError))
        },
        _ = connection_timeout => {
            // connection_timeout completed before we got any 'Connection' packet. From the MQTT v5 specification:
            //   If the Server does not receive a CONNECT packet within a reasonable amount
            //   of time after the Network Connection is established, the Server SHOULD close the Network Connection.
            Err(Box::new(ClientHandlerError::ConnectError))
        }
    }
}
