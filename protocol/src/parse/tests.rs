use crate::parse::MqttParserError::MalformedPacket;
use crate::parse::{parse_header, parse_mqtt, parse_string};
use crate::types::{Connect, ConnAck, ConnectReason, MqttPacket, Will};

macro_rules! variable_uint_tests {
    ($($name:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $name() -> Result<(), String> {
            let (input, expected) = $value;
            if let Ok((rest, header)) = parse_header(input) {
                assert_eq!(header.packet_size, expected);
                assert_eq!(rest.len(), 0usize);
                Ok(())
            } else {
                Err("failed.".to_string())
            }
        }
    )*
    }
}

// https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901011
variable_uint_tests! {
    one_lower: (&[0b1100_0000u8, 0b0000_0000u8], 0u32),
    one_upper: (&[0b1100_0000u8, 0x7fu8], 127u32),
    two_lower: (&[0b1100_0000u8, 0x80u8, 0x01u8], 128u32),
    two_upper: (&[0b1100_0000u8, 0xffu8, 0x7fu8], 16_383u32),
    three_lower: (&[0b1100_0000u8, 0x80u8, 0x80u8, 0x01u8], 16_384u32),
    three_upper: (&[0b1100_0000u8, 0xffu8, 0xffu8, 0x7fu8], 2_097_151u32),
    four_lower: (&[0b1100_0000u8, 0x80u8, 0x80u8, 0x80u8, 0x01u8], 2_097_152u32),
    four_upper: (&[0b1100_0000u8, 0xffu8, 0xffu8, 0xffu8, 0x07fu8], 268_435_455u32),
}

macro_rules! packet_tests {
    ($($name:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $name() -> Result<(), String> {
            let (input, expected) = $value;
            if let Ok((rest, packet)) = parse_mqtt(input) {
                assert_eq!(packet, expected);
                assert_eq!(rest.len(), 0usize);
                Ok(())
            } else {
                Err("failed.".to_string())
            }
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
            password: Some("PASS".to_string()),
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
            // Will Delay Interval
            24, 0, 0, 0, 1,
            // Message Expiry Interval
            2, 0, 0, 0, 1,
            // Content Type
            3, 0, 10, 112, 108, 97, 105, 110, 47, 116, 101, 120, 116,
            // Payload Format Indicator
            1, 1,

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
            password: Some("PASSWORD".to_string()),
            will: Some(Will {
                retain: false,
                qos: 0,
                topic: "foo".to_string(),
                payload: vec![116, 97, 108, 116, 97, 108],
                delay_interval: Some(1),
                payload_format_indicator: Some(1),
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
            user_properties: None })
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
    pingreq_with_non_empty_flags: (&[0b1100_0001u8, 0u8], nom::Err::Failure(MalformedPacket)),
}

macro_rules! string_tests {
    ($($name:ident: $input:expr,)*) => {
    $(
        #[test]
        fn $name() -> Result<(), String> {
            let input_as_bytes = $input.as_bytes();
            let length = (input_as_bytes.len() as u16).to_be_bytes();
            let mut v = vec![];
            v.extend_from_slice(&length);
            v.extend_from_slice(input_as_bytes);
            if let Ok((rest, result)) = parse_string(&v) {
                assert_eq!(&result, $input);
                assert_eq!(rest.len(), 0usize);
                Ok(())
            } else {
                Err(format!("failed to parse string: {}.", $input))
            }
        }
    )*
    }
}

string_tests! {
    short_string: "🚀",
    longer_string: "🚀longer string with spaces & weird signs ‰{¢‰",
}
