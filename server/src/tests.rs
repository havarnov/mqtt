use crate::{handle_connect, ClientIdentifier, HandleConnectError};
use futures::SinkExt;
use mqtt_protocol::codec::framed::MqttPacketDecoder;
use mqtt_protocol::types::{Connect, MqttPacket};
use std::error::Error;
use tokio::io::DuplexStream;
use tokio_util::codec::Framed;

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
            authentication_method: None,
            authentication_data: None,
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
async fn handle_connect_first_packet_not_connect() -> Result<(), Box<dyn Error>> {
    let (mut client_side, mut server_side) = create_client();

    client_side.send(MqttPacket::PingReq).await?;

    if let Err(HandleConnectError::ConnectNotFirstPacket) = handle_connect(&mut server_side).await {
        ()
    } else {
        assert!(
            false,
            "Should return error since first packet was not Connect."
        );
    };

    Ok(())
}
