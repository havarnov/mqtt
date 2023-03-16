use crate::session::MemorySessionProvider;
use crate::{handle_connect, ClientIdentifier, HandleConnectError, IncomingConnectionHandler};
use futures::{SinkExt, StreamExt};
use mqtt_protocol::codec::MqttPacketDecoder;
use mqtt_protocol::{Connect, MqttPacket};
use std::error::Error;
use tokio::io::DuplexStream;
use tokio::sync::broadcast;
use tokio_util::codec::Framed;

macro_rules! assert_fail {
    ($msg:expr) => {
        assert!(false, $msg);
        unreachable!("due to assert(false).")
    };
}

#[derive(Default)]
struct ConnectBuilder {
    pub client_identifier: Option<String>,
    pub clean_start: Option<bool>,
}

impl ConnectBuilder {
    fn new() -> ConnectBuilder {
        Default::default()
    }

    fn build(self) -> Result<Connect, Box<dyn std::error::Error>> {
        Ok(Connect {
            protocol_name: "MQTT".to_string(),
            protocol_version: 5u8,
            client_identifier: self
                .client_identifier
                .ok_or("client identifer not set".to_string())?,
            username: None,
            password: None,
            will: None,
            clean_start: self.clean_start.ok_or("clean_start not set".to_string())?,
            keep_alive: 0u16,
            session_expiry_interval: None,
            receive_maximum: None,
            maximum_packet_size: None,
            topic_alias_maximum: None,
            request_response_information: None,
            request_problem_information: None,
            user_properties: None,
            authentication: None,
        })
    }

    fn client_identifier(mut self, client_identifier: &str) -> Self {
        self.client_identifier = Some(client_identifier.to_string());
        self
    }

    fn clean_start(mut self, clean_start: bool) -> Self {
        self.clean_start = Some(clean_start);
        self
    }
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

#[tokio::test]
async fn handle_connect_server_assigned_client_identifier() -> Result<(), Box<dyn Error>> {
    let (mut client_side, mut server_side) = create_client();

    client_side
        .send(MqttPacket::Connect(
            ConnectBuilder::new()
                .client_identifier("")
                .clean_start(true)
                .build()?,
        ))
        .await?;

    let (client_identifier, _connect) = handle_connect(&mut server_side).await?;

    match client_identifier {
        ClientIdentifier::ServerAssigned { client_identifier } => {
            assert!(!client_identifier.is_empty())
        }
        _ => assert!(false, "ClientIdentifier should be Normal and match."),
    }

    Ok(())
}

#[tokio::test]
async fn handle_connect_normal_client_identifier() -> Result<(), Box<dyn Error>> {
    let (mut client_side, mut server_side) = create_client();

    client_side
        .send(MqttPacket::Connect(
            ConnectBuilder::new()
                .client_identifier("test")
                .clean_start(true)
                .build()?,
        ))
        .await?;

    let (client_identifier, _connect) = handle_connect(&mut server_side).await?;

    match client_identifier {
        ClientIdentifier::Normal { client_identifier } => assert_eq!("test", &client_identifier),
        _ => assert!(false, "ClientIdentifier should be Normal and match."),
    }

    Ok(())
}

#[tokio::test]
async fn connect_and_connack() -> Result<(), Box<dyn Error>> {
    let handler = IncomingConnectionHandler {
        clients: dashmap::DashMap::new(),
        session_provider: MemorySessionProvider::new(),
    };

    let (mut client_side, server_side) = create_client();

    client_side
        .send(MqttPacket::Connect(
            ConnectBuilder::new()
                .client_identifier("test")
                .clean_start(true)
                .build()?,
        ))
        .await?;

    let (broadcast_tx, _) = broadcast::channel(1000);
    if let Err(e) = handler.incoming_connection(server_side, broadcast_tx).await {
        println!("an error occurred; error = {:?}", e);
    }

    let Some(Ok(MqttPacket::ConnAck(_))) = client_side.next().await else {
        assert_fail!("First message from server must be ConnAck.");
    };

    Ok(())
}

#[tokio::test]
async fn multiple_connect_should_close_connection() -> Result<(), Box<dyn Error>> {
    let handler = IncomingConnectionHandler {
        clients: dashmap::DashMap::new(),
        session_provider: MemorySessionProvider::new(),
    };

    let (mut client_side, server_side) = create_client();

    client_side
        .send(MqttPacket::Connect(
            ConnectBuilder::new()
                .client_identifier("test")
                .clean_start(true)
                .build()?,
        ))
        .await?;

    let (broadcast_tx, _) = broadcast::channel(1000);
    if let Err(e) = handler.incoming_connection(server_side, broadcast_tx).await {
        println!("an error occurred; error = {:?}", e);
    }

    let Some(Ok(MqttPacket::ConnAck(_))) = client_side.next().await else {
        assert_fail!("First message from server must be ConnAck.");
    };

    client_side
        .send(MqttPacket::Connect(
            ConnectBuilder::new()
                .client_identifier("test")
                .clean_start(true)
                .build()?,
        ))
        .await?;

    let None = client_side.next().await else {
        assert_fail!("Server MUST close connection if client sends multiple Connect packets.");
    };

    Ok(())
}

#[tokio::test]
async fn handle_connect_first_packet_not_connect() -> Result<(), Box<dyn Error>> {
    let (mut client_side, mut server_side) = create_client();

    client_side.send(MqttPacket::PingReq).await?;

    if let Err(HandleConnectError::ConnectNotFirstPacket) = handle_connect(&mut server_side).await {
        ()
    } else {
        assert_fail!("Should return error since first packet was not Connect.");
    };

    Ok(())
}
