use std::collections::HashMap;
use futures::SinkExt;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use async_trait::async_trait;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{ConnAck, ConnectReason, MqttPacket, SubAck, SubscribeReason};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (tx, rx) = unbounded_channel();
    tokio::try_join!(connect_handler(rx), listener(tx)).map(|_| ())
}

#[async_trait]
trait Broker {
    async fn incoming_connect(self: &Self, packet_identifier: u16) -> oneshot::Receiver<()>;
}

struct StandardBroker {
    clients: RwLock<HashMap<u16, oneshot::Sender<()>>>,
}

#[async_trait]
impl Broker for StandardBroker {
    async fn incoming_connect(self: &Self, packet_identifier: u16) -> Receiver<()> {
        let mut clients = self.clients.write().expect("Read lock on clients");
        let (disconnect_tx, disconnect_rx) = oneshot::channel();
        clients.entry(packet_identifier).or_insert(disconnect_tx);
        disconnect_rx
    }
}

async fn connect_handler(
    mut rx: UnboundedReceiver<oneshot::Sender<()>>,
) -> Result<(), Box<dyn Error>> {
    while let Some(response_channel) = rx.recv().await {
        println!("got something");
        response_channel.send(()).expect("couldn't send.")
    }

    Ok(())
}

async fn listener(tx: UnboundedSender<Sender<()>>) -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:6142";
    let listener = TcpListener::bind(&addr).await?;
    loop {
        let t = tx.clone();
        // // Asynchronously wait for an inbound TcpStream.
        let (stream, addr) = listener.accept().await?;

        // // Clone a handle to the `Shared` state for the new connection.
        let state = Arc::new(Mutex::new(0u64));

        // // Spawn our handler to be run asynchronously.
        tokio::spawn(async move {
            println!("accepted connection");
            if let Err(e) = process(state, t, stream, addr).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

/// Process an individual chat client
async fn process(
    _state: Arc<Mutex<u64>>,
    tx: UnboundedSender<oneshot::Sender<()>>,
    stream: TcpStream,
    _addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut framed = Framed::new(stream, MqttPacketDecoder {});

    handle_connection(&mut framed, tx).await?;

    loop {
        match framed.next().await {
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
            }
            None => return Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
struct HandleConnectionError;

impl std::fmt::Display for HandleConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "something happened during connection.")
    }
}

impl std::error::Error for HandleConnectionError {}

async fn handle_connection(
    framed: &mut Framed<TcpStream, MqttPacketDecoder>,
    connect_tx: UnboundedSender<oneshot::Sender<()>>,
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

                        // connect_handler.incoming_client(c.client_identifier).await??;

                        let (oneshot_tx, oneshot_rx) = oneshot::channel();
                        connect_tx.send(oneshot_tx)?;

                        // allow HQ to use 10ms to process the CONNECT.
                        tokio::time::timeout(Duration::from_millis(10), oneshot_rx).await??;

                        return Ok(());
                }
                _ => return Err(Box::new(HandleConnectionError))
        },
        _ = connection_timeout => {
            // connection_timeout completed before we got any 'Connection' packet. From the MQTT v5 specification:
            //   If the Server does not receive a CONNECT packet within a reasonable amount
            //   of time after the Network Connection is established, the Server SHOULD close the Network Connection.
            return Err(Box::new(HandleConnectionError));
        }
    }
}
