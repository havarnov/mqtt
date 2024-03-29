use crate::codec::decoding::MqttParserError::MalformedPacket;
use crate::codec::decoding::{parse_mqtt, MqttParserError};
use crate::codec::encoding::encode;
use crate::types::{
    ConnAck, Connect, ConnectReason, Disconnect, DisconnectReason, MqttPacket, PubAck,
    PubAckReason, Publish, QoS, RetainHandling, SubAck, Subscribe, SubscribeReason, TopicFilter,
    UnsubAck, Unsubscribe, UnsubscribeReason, Will,
};
use crate::Payload;

macro_rules! packet_tests {
    ($($name:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $name() -> Result<(), nom::Err<MqttParserError<&'static [u8]>>> {
            let (input, expected) = $value;
            let (rest, packet) = parse_mqtt(input)?;
            assert_eq!(packet, expected);
            assert_eq!(rest.len(), 0usize);
            let encoded = encode(&packet);
            assert_eq!(&input[..], &encoded[..]);
            Ok(())
        }
    )*
    }
}

packet_tests! {
    pingreq: (
        &[0b1100_0000u8, 0u8],
        MqttPacket::PingReq
    ),

    pingresp: (
        &[0b1101_0000u8, 0u8],
        MqttPacket::PingResp
    ),

    simple_connect: (
        &[
            // Header
            0b0001_0000u8, 32u8,

            // Variable header
            // MQTT
            0u8,
            4u8,
            0b0100_1101,
            0b0101_0001,
            0b0101_0100,
            0b0101_0100,
            // protocol version
            5u8,
            // connect flags
            0b1100_0010,
            // keep alive
            0u8,
            10u8,
            // properties
            0u8,

            // Payload
            // client identifier: "testing"
            0u8, 7u8, 0x74u8, 0x65u8, 0x73u8, 0x74u8, 0x69u8, 0x6eu8, 0x67u8,
            // username: "USER"
            0u8, 4u8, 0x55u8, 0x53u8, 0x45u8, 0x52u8,
            // password: "PASS"
            0u8, 4u8, 0x50u8, 0x41u8, 0x53u8, 0x53u8,
        ],
        MqttPacket::Connect(Connect {
            protocol_name: "MQTT".to_string(),
            protocol_version: 5u8,
            client_identifier: "testing".to_string(),
            username: Some("USER".to_string()),
            password: Some("PASS".as_bytes().to_vec()),
            will: None,
            clean_start: true,
            keep_alive: 10u16,
            session_expiry_interval: None,
            receive_maximum: None,
            maximum_packet_size: None,
            topic_alias_maximum: None,
            request_response_information: None,
            request_problem_information: None,
            user_properties: None,
            authentication: None,
        })
    ),

    connect_with_properties: (
        &[
            // -- Fixed header --
            16,
            96,

            // -- Variable header --
            // MQTT
            0, 4, 77, 81, 84, 84, 5,

            // Connect flags
            198,

            // Keep alive
            0, 10,

            // Properties
            11, // length
            // Session Expiry Interval
            17, 0, 0, 0, 11,
            // Receive Maximum
            33, 0, 10,
            // Topic Alias Maximum
            34, 0, 10,

            // -- Payload --
            // Client identifier
            0, 13, 104, 97, 118, 97, 114, 45, 116, 101, 115, 116, 105, 110, 103,

            // Will properties
            25, // length
            // Payload Format Indicator
            1, 1,
            // Message Expiry Interval
            2, 0, 0, 0, 1,
            // Content Type
            3, 0, 10, 112, 108, 97, 105, 110, 47, 116, 101, 120, 116,
            // Will Delay Interval
            24, 0, 0, 0, 1,

            // Will topic
            0, 3, 102, 111, 111,

            // Will payload
            0, 6, 116, 97, 108, 116, 97, 108,

            // User name
            0, 8, 85, 83, 69, 82, 78, 65, 77, 69,

            // Password
            0, 8, 80, 65, 83, 83, 87, 79, 82, 68
        ],
        MqttPacket::Connect(Connect {
            protocol_name: "MQTT".to_string(),
            protocol_version: 5,
            client_identifier: "havar-testing".to_string(),
            username: Some("USERNAME".to_string()),
            password: Some("PASSWORD".as_bytes().to_vec()),
            will: Some(Will {
                retain: false,
                qos: QoS::AtMostOnce,
                topic: "foo".to_string(),
                payload: Payload::String("taltal".to_string()),
                delay_interval: Some(1),
                message_expiry_interval: Some(1),
                content_type: Some("plain/text".to_string()),
                response_topic: None,
                correlation_data: None,
                user_properties: None }),
            clean_start: true,
            keep_alive: 10,
            session_expiry_interval: Some(11),
            receive_maximum: Some(10),
            maximum_packet_size: None,
            topic_alias_maximum: Some(10),
            request_response_information: None,
            request_problem_information: None,
            user_properties: None,
            authentication: None})
    ),

    simple_connack: (
        &[
            // -- Fixed header --
            0b0010_0000u8,
            3u8, // packet length

            // -- Variable header --
            // Connect Acknowledge flags
            0b0000_00000u8,
            // Connect Reason Code
            0u8, // Success
            // Properties
            0u8, // property length
        ],
        MqttPacket::ConnAck(ConnAck {
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
            wildcard_subscription_available: None,
            subscription_identifiers_available: None,
            shared_subscription_available: None,
            server_keep_alive: None,
            response_information: None,
            server_reference: None,
            authentication_method: None,
            authentication_data: None,
        })
    ),

    subscribe : (
        &[
            // -- Fixed header --
            0b1000_0010u8,
            21u8, // packet length

            // -- Variable header --
            // Packet identifier
            0u8,
            1u8,
            // No user properties
            // Properties
            2u8, // property length
            11u8, // subscription identifier property identifier
            2u8, // subscription identifier

            // -- Payload --
            // (1 or multiple topic filter names as ut8 strings + 1 option byte)
            // foobar
            0u8, 6u8, 0x66, 0x6F, 0x6F, 0x62, 0x61, 0x72,
            0b0000_0100,

            // rall
            0u8, 4u8, 0x72, 0x61, 0x6C, 0x6C,
            0b0001_1010,
        ],
        MqttPacket::Subscribe(Subscribe {
            packet_identifier: 1u16,
            subscription_identifier: Some(2u32),
            user_properties: None,
            topic_filters: vec![
                TopicFilter {
                    filter: "foobar".to_string(),
                    maximum_qos: QoS::AtMostOnce,
                    no_local: true,
                    retain_handling: RetainHandling::SendRetained,
                    retain_as_published: false,
                },
                TopicFilter {
                    filter: "rall".to_string(),
                    maximum_qos: QoS::ExactlyOnce,
                    no_local: false,
                    retain_handling: RetainHandling::SendRetainedForNewSubscription,
                    retain_as_published: true,
                },
            ]
        }),
    ),

    unsubscribe: (
        &[
            // -- Fixed header --
            0b1010_0000u8,
            11u8, // packet length

            // -- Variable header --
            // Packet identifier
            0u8,
            1u8,
            // No user properties
            // Properties
            0u8, // property length

            // -- Payload --
            // (1 or multiple topic filter names as ut8 strings)
            // foobar
            0u8, 6u8, 0x66, 0x6F, 0x6F, 0x62, 0x61, 0x72
        ],
        MqttPacket::Unsubscribe(Unsubscribe {
            packet_identifier: 1u16,
            user_properties: None,
            topic_filters: vec!["foobar".to_string()]
        })
    ),

    publish_qos0: (
        &[
            // -- Fixed header --
            0b0011_1001u8,
            12u8, // packet length

            // -- Variable header --
            // topic name (foobar)
            0u8, 6u8, 0x66, 0x6F, 0x6F, 0x62, 0x61, 0x72,
            // Properties
            0u8, // property length

            // -- Payload --
            1u8, 3u8, 5u8,
        ],
        MqttPacket::Publish(Publish {
            duplicate: true,
            qos: QoS::AtMostOnce,
            retain: true,
            topic_name: "foobar".to_string(),
            packet_identifier: None,
            message_expiry_interval: None,
            topic_alias: None,
            response_topic: None,
            correlation_data: None,
            user_properties: None,
            subscription_identifier: None,
            content_type: None,
            payload: Payload::Unspecified(vec![1u8, 3u8, 5u8]),
        }),
    ),

    publish: (
        &[
            // -- Fixed header --
            0b0011_1011u8,
            14u8, // packet length

            // -- Variable header --
            // topic name (foobar)
            0u8, 6u8, 0x66, 0x6F, 0x6F, 0x62, 0x61, 0x72,
            // packet identifier
            0u8, 42u8,
            // Properties
            0u8, // property length

            // -- Payload --
            1u8, 3u8, 5u8,
        ],
        MqttPacket::Publish(Publish {
            duplicate: true,
            qos: QoS::AtLeastOnce,
            retain: true,
            topic_name: "foobar".to_string(),
            packet_identifier: Some(42u16),
            message_expiry_interval: None,
            topic_alias: None,
            response_topic: None,
            correlation_data: None,
            user_properties: None,
            subscription_identifier: None,
            content_type: None,
            payload: Payload::Unspecified(vec![1u8, 3u8, 5u8]),
        }),
    ),

    suback: (
        &[
            // -- Fixed header --
            0b1001_0000u8,
            16u8, // packet length

            // -- Variable header --
            // packet identifier
            0u8, 42u8,
            // Properties
            12u8, // property length
            0x1f, // reason string identifier
            0u8, 9u8, 0x6e, 0x6f, 0x20, 0x72, 0x65, 0x61, 0x73, 0x6f, 0x6e,

            // -- Payload --
            0u8,
        ],
        MqttPacket::SubAck(SubAck {
            packet_identifier: 42u16,
            reason_string: Some("no reason".to_string()),
            user_properties: None,
            reasons: vec![SubscribeReason::GrantedQoS0],
        }),
    ),

    disconnect: (
        &[
            // -- Fixed header --
            0b1110_0000u8,
            2u8, // packet length

            // -- Variable header --
            // disconnect reason code
            0u8,
            // Properties
            0u8, // property length
        ],
        MqttPacket::Disconnect(Disconnect {
            disconnect_reason: DisconnectReason::NormalDisconnection,
            session_expiry_interval: None,
            reason_string: None,
            user_properties: None,
            server_reference: None,
        }),
    ),

    unsuback: (
        &[
            // -- Fixed header --
            0b1011_0000u8,
            4u8, // packet length

            // -- Variable header --
            // packet identifier
            0u8, 42u8,
            // Properties
            0u8, // property length

            // -- Payload --
            0u8
        ],
        MqttPacket::UnsubAck(UnsubAck {
            packet_identifier: 42u16,
            reason_string: None,
            user_properties: None,
            reasons: vec![UnsubscribeReason::Success],
        })
    ),

    puback_simple: (
        &[
            // -- Fixed header --
            0b0100_0000u8,
            2u8,

            // -- Variable header --
            // packet identifier (256)
            1u8, 0u8,
        ],
        MqttPacket::PubAck(PubAck {
            packet_identifier: 256u16,
            reason: PubAckReason::Success,
            reason_string: None,
            user_properties: None,
        })
    ),

    puback: (
        &[
            // -- Fixed header --
            0b0100_0000u8,
            13u8,

            // -- Variable header --
            // packet identifier (256)
            1u8, 0u8,
            // reason
            153u8,

            // -- Properties --
            // property length
            9u8,

            0x1fu8,
            0u8, 6u8, 0x66, 0x6f, 0x6f, 0x62, 0x61, 0x72,

        ],
        MqttPacket::PubAck(PubAck {
            packet_identifier: 256u16,
            reason: PubAckReason::PayloadFormatInvalid,
            reason_string: Some("foobar".to_string()),
            user_properties: None,
        })
    ),
}

macro_rules! parsing_should_fail_tests {
    ($($name:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $name() -> Result<(), String> {
            let (input, expected) = $value;
            match parse_mqtt(input) {
                Err(e) => {
                    assert_eq!(e, expected);
                    Ok(())
                },
                _ => Err("parsing should fail".to_string()),
            }
        }
    )*
    }
}

parsing_should_fail_tests! {
    pingreq_with_non_empty_flags: (&[0b1100_0001u8, 0u8], nom::Err::Failure(MalformedPacket("Parsing PingReq: can't have flags or packet size.".to_string()))),
    publish_qos1_packet_identifier_0: (
        &[
            // -- Fixed header --
            0b0011_1011u8,
            14u8, // packet length

            // -- Variable header --
            // topic name (foobar)
            0u8, 6u8, 0x66, 0x6F, 0x6F, 0x62, 0x61, 0x72,
            // packet identifier
            0u8, 0u8,
            // Properties
            0u8, // property length

            // -- Payload --
            1u8, 3u8, 5u8,
        ],
        nom::Err::Failure(MalformedPacket("QoS >= 1 requires non zero Packet Identifier.".to_string())),
    ),
}
