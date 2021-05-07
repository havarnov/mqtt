mod decoder;
mod parser;
mod protocol;

use futures::SinkExt;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::Framed;

use crate::protocol::{ConnAck, ConnectReason, MqttPacket, SubAck, SubscribeReason};
use decoder::MqttPacketDecoder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:6142";
    let listener = TcpListener::bind(&addr).await?;
    loop {
        // // Asynchronously wait for an inbound TcpStream.
        let (stream, addr) = listener.accept().await?;

        // // Clone a handle to the `Shared` state for the new connection.
        let state = Arc::new(Mutex::new(0u64));

        // // Spawn our handler to be run asynchronously.
        tokio::spawn(async move {
            println!("accepted connection");
            if let Err(e) = process(state, stream, addr).await {
                println!("an error occurred; error = {:?}", e);
            }
        });
    }
}

/// Process an individual chat client
async fn process(
    _state: Arc<Mutex<u64>>,
    stream: TcpStream,
    _addr: SocketAddr,
) -> Result<(), Box<dyn Error>> {
    let mut framed = Framed::new(stream, MqttPacketDecoder {});

    loop {
        match framed.next().await {
            Some(Ok(MqttPacket::Connect(c))) => {
                println!("{:?}", c);
                framed
                    .send(MqttPacket::ConnAck(ConnAck {
                        session_present: false,
                        connect_reason: ConnectReason::Success,
                        properties: Default::default(),
                    }))
                    .await?;
            }
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
                        properties: Default::default(),
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
        };
    }
}
