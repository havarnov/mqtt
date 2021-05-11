use std::str;

use nom::bits::bits;
use nom::bits::streaming::take as bit_take;
use nom::bytes::streaming::take;
use nom::combinator::{map, map_parser};
use nom::error::Error;
use nom::number::streaming::{u16, u32};
use nom::number::Endianness;
use nom::sequence::tuple;
use nom::IResult;

use crate::types::{
    Connect, Disconnect, DisconnectReason, MqttPacket, Properties, Publish, QoS, Subscribe,
    TopicFilter, UserProperty, Will,
};
use MqttParserError::*;

#[derive(Debug, PartialEq)]
pub enum MqttParserError<I> {
    MalformedPacket,
    Nom(I, nom::error::ErrorKind),
}

impl<I> nom::error::ParseError<I> for MqttParserError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        MqttParserError::Nom(input, kind)
    }

    fn append(_: I, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

type MqttParserResult<I, O> = IResult<I, O, MqttParserError<I>>;

struct MqttHeader {
    packet_type: u8,
    flags: u8,
    packet_size: u32,
}

fn parse_first_byte(input: (&[u8], usize)) -> IResult<(&[u8], usize), (u8, u8)> {
    let (rest, fst) = bit_take::<_, _, _, Error<_>>(4usize)(input)?;
    let (rest, snd) = bit_take::<_, _, _, Error<_>>(4usize)(rest)?;
    Ok((rest, (fst, snd)))
}

fn parse_variable_u32(input: &[u8]) -> MqttParserResult<&[u8], u32> {
    let mut rest = input;

    let mut result = 0u32;

    let mut shift = 0;
    loop {
        let (rest_next, size) = take(1usize)(rest)?;
        rest = rest_next;

        if size[0] >= 128u8 {
            result += (size[0] as u32 - 128u32) << shift * 7;
            shift += 1;
        } else {
            result += (size[0] as u32) << 7 * shift;
            break Ok((rest, result));
        }
    }
}

fn parse_string(input: &[u8]) -> MqttParserResult<&[u8], String> {
    let (rest, length) = u16(Endianness::Big)(input)?;
    let (rest, string_bytes) = take(length)(rest)?;
    match str::from_utf8(string_bytes) {
        Ok(str) => Ok((rest, str.to_owned())),
        Err(_) => Err(nom::Err::Failure(MalformedPacket)),
    }
}

fn parse_string_pair(input: &[u8]) -> MqttParserResult<&[u8], (String, String)> {
    let (input, key) = parse_string(input)?;
    let (input, value) = parse_string(input)?;
    Ok((input, (key, value)))
}

fn parse_header(input: &[u8]) -> MqttParserResult<&[u8], MqttHeader> {
    let (rest, (packet_type, flags)) =
        match bits::<_, _, _, nom::error::Error<_>, _>(parse_first_byte)(input) {
            Ok(res) => Ok(res),
            Err(nom::Err::Failure(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket))
            }
            Err(nom::Err::Error(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket))
            }
            Err(nom::Err::Incomplete(n)) => return Err(nom::Err::Incomplete(n)),
        }?;

    let (rest, packet_size) = parse_variable_u32(rest)?;

    Ok((
        rest,
        MqttHeader {
            packet_type,
            flags,
            packet_size,
        },
    ))
}

fn parse_binary_data(input: &[u8]) -> MqttParserResult<&[u8], &[u8]> {
    let (rest, length) = u16(Endianness::Big)(input)?;
    take(length)(rest)
}

fn map_to_property<T, I>(option: &mut Option<T>, value: (I, T)) -> MqttParserResult<I, ()> {
    if option.is_some() {
        Err(nom::Err::Failure(MalformedPacket))
    } else {
        *option = Some(value.1);
        Ok((value.0, ()))
    }
}

pub fn parse_u8_as_bool(input: &[u8]) -> IResult<&[u8], bool, MqttParserError<&[u8]>> {
    match input[0] {
        0u8 => Ok((input, false)),
        1u8 => Ok((input, true)),
        _ => Err(nom::Err::Failure(MalformedPacket)),
    }
}

fn parse_properties(input: &[u8]) -> MqttParserResult<&[u8], Properties> {
    let (mut input, mut properties_length) = parse_variable_u32(input)?;

    let mut props = Properties::new();

    // let mut properties = vec![];
    while properties_length > 0 {
        let (rest_next, property_identifier) = parse_variable_u32(input)?;
        let (rest_next, _) = match property_identifier {
            1u32 => map_to_property(
                &mut props.payload_format_indicator,
                map(take(1usize), |b: &[u8]| b[0])(rest_next)?,
            ),
            2u32 => map_to_property(
                &mut props.message_expiry_interval,
                u32(Endianness::Big)(rest_next)?,
            ),
            3u32 => map_to_property(&mut props.content_type, parse_string(rest_next)?),
            8u32 => map_to_property(&mut props.response_topic, parse_string(rest_next)?),
            9u32 => map_to_property(
                &mut props.correlation_data,
                map(parse_binary_data, |b| b.to_vec())(rest_next)?,
            ),
            17u32 => map_to_property(
                &mut props.session_expiry_interval,
                u32(Endianness::Big)(rest_next)?,
            ),
            21u32 => map_to_property(&mut props.authentication_method, parse_string(rest_next)?),
            22u32 => map_to_property(
                &mut props.authentication_data,
                map(parse_binary_data, |b| b.to_vec())(rest_next)?,
            ),
            23u32 => map_to_property(
                &mut props.request_problem_information,
                map_parser(take(1usize), parse_u8_as_bool)(rest_next)?,
            ),
            24u32 => map_to_property(
                &mut props.will_delay_interval,
                u32(Endianness::Big)(rest_next)?,
            ),
            25u32 => map_to_property(
                &mut props.request_response_information,
                map_parser(take(1usize), parse_u8_as_bool)(rest_next)?,
            ),
            33u32 => map_to_property(&mut props.receive_maximum, u16(Endianness::Big)(rest_next)?),
            34u32 => map_to_property(
                &mut props.topic_alias_maximum,
                u16(Endianness::Big)(rest_next)?,
            ),
            38u32 => {
                let (rest, s) = parse_string_pair(rest_next)?;
                props
                    .user_property
                    .get_or_insert(vec![])
                    .push(UserProperty {
                        key: s.0,
                        value: s.1,
                    });
                Ok((rest, ()))
            }
            39u32 => map_to_property(
                &mut props.maximum_packet_size,
                u32(Endianness::Big)(rest_next)?,
            ),
            code => {
                println!("Received property code: {:?}", code);
                Err(nom::Err::Failure(MalformedPacket))
            }
        }?;
        properties_length -= (input.len() - rest_next.len()) as u32;
        input = rest_next;
    }

    Ok((input, props))
}

fn parse_connect(input: &[u8]) -> MqttParserResult<&[u8], Connect> {
    let (input, protocol_name) = parse_string(input)?;
    if protocol_name != "MQTT" {
        return Err(nom::Err::Failure(MalformedPacket));
    }

    let (input, protocol_version) = take(1usize)(input)?;
    let protocol_version = protocol_version[0];

    let (input, (user_name_flag, password_flag, will_retain, will_qos, will_flag, clean_start)) =
        match bits::<_, (u8, u8, u8, u8, u8, u8), _, nom::error::Error<_>, _>(tuple((
            bit_take::<_, _, _, Error<_>>(1usize),
            bit_take::<_, _, _, Error<_>>(1usize),
            bit_take::<_, _, _, Error<_>>(1usize),
            bit_take::<_, _, _, Error<_>>(2usize),
            bit_take::<_, _, _, Error<_>>(1usize),
            bit_take::<_, _, _, Error<_>>(1usize),
        )))(input)
        {
            Ok(res) => Ok(res),
            Err(nom::Err::Failure(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket))
            }
            Err(nom::Err::Error(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket))
            }
            Err(nom::Err::Incomplete(n)) => return Err(nom::Err::Incomplete(n)),
        }?;

    let (input, keep_alive) = u16(Endianness::Big)(input)?;

    let (input, properties) = parse_properties(input)?;

    let (input, client_identifier) = parse_string(input)?;

    let (input, will) = if will_flag != 0u8 {
        let (rest, will_properties) = parse_properties(input)?;
        let (rest, will_topic) = parse_string(rest)?;
        let (rest, will_payload) = parse_binary_data(rest)?;

        (
            rest,
            Some(Will {
                retain: will_retain != 0u8,
                qos: will_qos,
                topic: will_topic,
                payload: will_payload.to_vec(),
                delay_interval: will_properties.will_delay_interval,
                payload_format_indicator: will_properties.payload_format_indicator,
                message_expiry_interval: will_properties.message_expiry_interval,
                content_type: will_properties.content_type,
                response_topic: will_properties.response_topic,
                correlation_data: will_properties.correlation_data,
                user_properties: will_properties.user_property,
            }),
        )
    } else {
        (input, None)
    };

    let (input, username) = if user_name_flag != 0u8 {
        map(parse_string, Some)(input)?
    } else {
        (input, None)
    };

    let (input, password) = if password_flag != 0u8 {
        map(parse_string, Some)(input)?
    } else {
        (input, None)
    };

    Ok((
        input,
        Connect {
            protocol_name,
            protocol_version,
            client_identifier,
            username,
            password,
            will,
            clean_start: clean_start != 0u8,
            keep_alive,
            session_expiry_interval: properties.session_expiry_interval,
            receive_maximum: properties.receive_maximum,
            maximum_packet_size: properties.maximum_packet_size,
            topic_alias_maximum: properties.topic_alias_maximum,
            request_response_information: properties.request_response_information,
            request_problem_information: properties.request_problem_information,
            user_properties: properties.user_property,
        },
    ))
}

fn parse_disconnect(input: &[u8]) -> MqttParserResult<&[u8], Disconnect> {
    let (input, disconnect_reason) = take(1usize)(input)?;
    let disconnect_reason = match disconnect_reason[0] {
        0u8 => DisconnectReason::NormalDisconnection,
        _ => unimplemented!(),
    };
    let (input, properties) = parse_properties(input)?;
    Ok((
        input,
        Disconnect {
            disconnect_reason,
            properties,
        },
    ))
}

fn parse_publish(packet_size: u32, flags: u8, input: &[u8]) -> MqttParserResult<&[u8], Publish> {
    let duplicate = flags & 0b0000_1000 == 0b0000_1000;
    let qos = match flags & 0b0000_01100 >> 1 {
        0u8 => QoS::AtMostOnce,
        1u8 => QoS::AtLeastOnce,
        2u8 => QoS::ExactlyOnce,
        _ => return Err(nom::Err::Failure(MalformedPacket)),
    };
    let retain = flags & 0b0000_0001 == 0b0000_00001;

    let len = input.len();
    let (input, topic_name) = parse_string(input)?;
    let (input, packet_identifier) = if qos != QoS::AtMostOnce {
        map(u16(Endianness::Big), Some)(input)?
    } else {
        (input, None)
    };
    let (input, properties) = parse_properties(input)?;
    let variable_header_length = (len - input.len()) as u32;
    let (input, payload) = take(packet_size - variable_header_length)(input)?;
    Ok((
        input,
        Publish {
            duplicate,
            qos,
            retain,
            topic_name,
            packet_identifier,
            properties,
            payload: payload.to_vec(),
        },
    ))
}

fn parse_subscribe(packet_size: u32, input: &[u8]) -> MqttParserResult<&[u8], Subscribe> {
    let len = input.len();
    let (input, packet_identifier) = u16(Endianness::Big)(input)?;
    let (mut input, properties) = parse_properties(input)?;
    let mut remaining = packet_size - (len - input.len()) as u32;
    let mut topic_filters = vec![];
    while remaining > 0 {
        let len = input.len();
        let (input_tmp, topic_name) = parse_string(input)?;
        let (input_tmp, options) = take(1usize)(input_tmp)?;

        let maximum_qos = match options[0] & 0b0000_0011 {
            0u8 => QoS::AtMostOnce,
            1u8 => QoS::AtLeastOnce,
            2u8 => QoS::ExactlyOnce,
            _ => return Err(nom::Err::Failure(MalformedPacket)),
        };

        let no_local = options[0] & 0b0000_0100 == 0b0000_0100;
        let retain_as_published = options[0] & 0b0000_1000 == 0b0000_1000;
        let retain_handling = options[0] & 0b0011_0000 >> 4;

        topic_filters.push(TopicFilter {
            topic_name,
            maximum_qos,
            no_local,
            retain_as_published,
            retain_handling,
        });

        input = input_tmp;
        remaining -= (len - input.len()) as u32;
    }

    Ok((
        input,
        Subscribe {
            packet_identifier,
            properties,
            topic_filters,
        },
    ))
}

pub fn parse_mqtt(input: &[u8]) -> MqttParserResult<&[u8], MqttPacket> {
    let (rest, header) = parse_header(input)?;

    match header.packet_type {
        1u8 => map(parse_connect, MqttPacket::Connect)(rest),
        3u8 => parse_publish(header.packet_size, header.flags, rest)
            .map(|(r, publish)| (r, MqttPacket::Publish(publish))),
        8u8 => parse_subscribe(header.packet_size, rest)
            .map(|(r, subscribe)| (r, MqttPacket::Subscribe(subscribe))),
        12u8 => {
            if header.flags != 0u8 {
                Err(nom::Err::Failure(MalformedPacket))
            } else {
                Ok((rest, MqttPacket::PingReq))
            }
        }
        13u8 => {
            if header.flags != 0u8 {
                Err(nom::Err::Failure(MalformedPacket))
            } else {
                Ok((rest, MqttPacket::PingResp))
            }
        }
        14u8 => map(parse_disconnect, MqttPacket::Disconnect)(rest),
        _ => Err(nom::Err::Failure(MalformedPacket)),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::MqttParserError::MalformedPacket;
    use crate::parse::{parse_header, parse_mqtt, parse_string};
    use crate::types::{Connect, MqttPacket, Will};

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
}
