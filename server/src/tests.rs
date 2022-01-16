use crate::{handle_connect, ClientIdentifier};
use futures::SinkExt;
use mqtt_protocol::framed::MqttPacketDecoder;
use mqtt_protocol::types::{Connect, MqttPacket};
use std::error::Error;
use tokio_util::codec::Framed;

#[tokio::test]
async fn handle_connect_normal_client_identifier() -> Result<(), Box<dyn Error>> {
    let (client, server) = tokio::io::duplex(4096);
    let mut client = Framed::new(client, MqttPacketDecoder {});
    let mut server = Framed::new(server, MqttPacketDecoder {});

    let handle_connect_future = handle_connect(&mut server);

    client
        .send(MqttPacket::Connect(Connect {
            protocol_name: "MQTT".to_string(),
            protocol_version: 5u8,
            client_identifier: "test".to_string(),
            username: None,
            password: None,
            will: None,
            clean_start: true,
            keep_alive: 1000,
            session_expiry_interval: None,
            receive_maximum: None,
            maximum_packet_size: None,
            topic_alias_maximum: None,
            request_response_information: None,
            request_problem_information: None,
            user_properties: None,
            authentication_method: None,
            authentication_data: None,
        }))
        .await?;

    let (client_identifier, connect) = handle_connect_future.await?;

    match client_identifier {
        ClientIdentifier::Normal { client_identifier } => assert_eq!("test", &client_identifier),
        _ => assert!(false, "ClientIdentifier should be Normal and match."),
    }

    assert_eq!(1000, connect.keep_alive);

    Ok(())
}
