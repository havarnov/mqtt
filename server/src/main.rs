use async_trait::async_trait;
use futures::SinkExt;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{
    ConnAck, ConnectReason, Disconnect, DisconnectReason, MqttPacket, SubAck, SubscribeReason,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let broker = Arc::new(StandardBroker {
        clients: RwLock::new(HashMap::new()),
    });
    listener(broker.clone()).await
}

#[async_trait]
trait Broker {
    async fn incoming_connect(
        &self,
        client_identifier: &str,
    ) -> oneshot::Receiver<oneshot::Sender<()>>;
}

struct StandardBroker {
    clients: RwLock<HashMap<String, oneshot::Sender<oneshot::Sender<()>>>>,
}

#[async_trait]
impl Broker for StandardBroker {
    #[allow(clippy::async_yields_async)]
    async fn incoming_connect(
        &self,
        client_identifier: &str,
    ) -> oneshot::Receiver<oneshot::Sender<()>> {
        let mut clients = self.clients.write().await;

        let (disconnect_tx, disconnect_rx) = oneshot::channel();
        if let Some(tx) = clients.insert(client_identifier.to_string(), disconnect_tx) {
            let (disconnect_tx, disconnect_rx) = oneshot::channel();
            tx.send(disconnect_tx).expect("disconnect_tx broker side.");
            disconnect_rx.await.expect("disconnect_rx broker side.");
        }

        disconnect_rx
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

/// Process an individual chat client
async fn process<B: Broker>(
    broker: Arc<B>,
    stream: TcpStream,
    _addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut framed = Framed::new(stream, MqttPacketDecoder {});

    let mut disconnect_rx = handle_connect(&mut framed, broker.deref()).await?;

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
                    }
                    Some(Ok(MqttPacket::Subscribe(subscribe))) => {
                        println!("client subscribed to: {:?}", subscribe.topic_filters);
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
            disconnect_tx = &mut disconnect_rx => {
                framed.send(MqttPacket::Disconnect(Disconnect {
                    disconnect_reason: DisconnectReason::SessionTakenOver,
                    server_reference: None,
                    session_expiry_interval: None,
                    user_properties: None,
                    reason_string: None,
                }))
                .await?;

                disconnect_tx?.send(()).map_err(|_| Box::new(ClientHandlerError::DisconnectError))?;
                return Ok(())
            }
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
) -> Result<oneshot::Receiver<oneshot::Sender<()>>, Box<dyn Error>> {
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

                        let disconnect_rx = broker.incoming_connect(&c.client_identifier).await;

                        Ok(disconnect_rx)
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
