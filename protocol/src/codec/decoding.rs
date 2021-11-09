use crate::codec::decoding::MqttParserError::MalformedPacket;
use crate::types::{
    ConnAck, Connect, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Properties, Publish,
    QoS, RetainHandling, SubAck, Subscribe, SubscribeReason, TopicFilter, UnsubAck, Unsubscribe,
    UnsubscribeReason, UserProperty, Will,
};
use nom::bits::bits;
use nom::bits::streaming::take as bit_take;
use nom::bytes::streaming::take;
use nom::combinator::{map, map_parser};
use nom::error::Error;
use nom::number::streaming::{u16, u32};
use nom::number::Endianness;
use nom::sequence::tuple;
use nom::{IResult, Needed};
use std::num::NonZeroUsize;
use std::str;

#[derive(Debug, PartialEq)]
pub enum MqttParserError<I> {
    MalformedPacket(String),
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

pub(in crate::codec) struct MqttHeader {
    pub(in crate::codec) packet_type: u8,
    pub(in crate::codec) flags: u8,
    pub(in crate::codec) packet_size: u32,
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
            result += (size[0] as u32 - 128u32) << (shift * 7);
            shift += 1;
        } else {
            result += (size[0] as u32) << (shift * 7);
            break Ok((rest, result));
        }
    }
}

pub(in crate::codec) fn parse_string(input: &[u8]) -> MqttParserResult<&[u8], String> {
    let (rest, length) = u16(Endianness::Big)(input)?;
    let (rest, string_bytes) = take(length)(rest)?;
    match str::from_utf8(string_bytes) {
        Ok(str) => Ok((rest, str.to_owned())),
        Err(_) => Err(nom::Err::Failure(MalformedPacket(
            "String is not valid UTF-8".to_owned(),
        ))),
    }
}

fn parse_string_pair(input: &[u8]) -> MqttParserResult<&[u8], (String, String)> {
    let (input, key) = parse_string(input)?;
    let (input, value) = parse_string(input)?;
    Ok((input, (key, value)))
}

pub(in crate::codec) fn parse_header(input: &[u8]) -> MqttParserResult<&[u8], MqttHeader> {
    let (rest, (packet_type, flags)) =
        match bits::<_, _, _, nom::error::Error<_>, _>(parse_first_byte)(input) {
            Ok(res) => Ok(res),
            Err(nom::Err::Failure(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket(
                    "First byte of header is malformed".to_owned(),
                )));
            }
            Err(nom::Err::Error(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket(
                    "First byte of header is malformed".to_owned(),
                )));
            }
            Err(nom::Err::Incomplete(n)) => return Err(nom::Err::Incomplete(n)),
        }?;

    let (rest, packet_size) = parse_variable_u32(rest)?;

    if packet_size > rest.len() as u32 {
        return Err(nom::Err::Incomplete(Needed::Size(
            NonZeroUsize::new((packet_size as usize) - rest.len())
                .expect("packet_size should be larger than rest.len()."),
        )));
    }

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
        Err(nom::Err::Failure(MalformedPacket(
            "Property is already set.".to_string(),
        )))
    } else {
        *option = Some(value.1);
        Ok((value.0, ()))
    }
}

fn parse_u8_as_bool(input: &[u8]) -> IResult<&[u8], bool, MqttParserError<&[u8]>> {
    match input[0] {
        0u8 => Ok((input, false)),
        1u8 => Ok((input, true)),
        _ => Err(nom::Err::Failure(MalformedPacket(
            "Only 0|1 are valid when parsing u8 to bool.".to_owned(),
        ))),
    }
}

fn parse_properties(input: &[u8]) -> MqttParserResult<&[u8], Properties> {
    if input.is_empty() {
        return Ok((input, Properties::default()));
    }

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
            11u32 => map_to_property(
                &mut props.subscription_identifier,
                parse_variable_u32(rest_next)?,
            ),
            17u32 => map_to_property(
                &mut props.session_expiry_interval,
                u32(Endianness::Big)(rest_next)?,
            ),
            18u32 => map_to_property(
                &mut props.retain_available,
                map_parser(take(1usize), parse_u8_as_bool)(rest_next)?,
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
            31u32 => map_to_property(&mut props.reason_string, parse_string(rest_next)?),
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
            code => Err(nom::Err::Failure(MalformedPacket(format!(
                "Received property code: {:?}",
                code
            )))),
        }?;

        // TODO: check length for overflow (malformed packet)
        properties_length -= (input.len() - rest_next.len()) as u32;
        input = rest_next;
    }

    Ok((input, props))
}

fn parse_connect(input: &[u8]) -> MqttParserResult<&[u8], Connect> {
    let (input, protocol_name) = parse_string(input)?;
    if protocol_name != "MQTT" {
        return Err(nom::Err::Failure(MalformedPacket(
            "Only 'MQTT' is valid protocol name.".to_owned(),
        )));
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
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket(
                    "Parsing first byte of Connect failed.".to_owned(),
                )));
            }
            Err(nom::Err::Error(_)) => {
                return Err(nom::Err::Failure(MqttParserError::MalformedPacket(
                    "Parsing first byte of Connect failed.".to_owned(),
                )));
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
            authentication_method: properties.authentication_method,
            authentication_data: properties.authentication_data,
        },
    ))
}

fn parse_disconnect(input: &[u8]) -> MqttParserResult<&[u8], Disconnect> {
    if input.is_empty() {
        return Ok((
            input,
            Disconnect {
                disconnect_reason: DisconnectReason::NormalDisconnection,
                session_expiry_interval: None,
                reason_string: None,
                user_properties: None,
                server_reference: None,
            },
        ));
    }

    let (input, disconnect_reason) = take(1usize)(input)?;
    let disconnect_reason = match disconnect_reason[0] {
        0u8 => DisconnectReason::NormalDisconnection,
        142u8 => DisconnectReason::SessionTakenOver,
        _ => unimplemented!(),
    };
    let (input, properties) = parse_properties(input)?;
    Ok((
        input,
        Disconnect {
            disconnect_reason,
            session_expiry_interval: properties.session_expiry_interval,
            reason_string: properties.reason_string,
            user_properties: properties.user_property,
            server_reference: properties.server_reference,
        },
    ))
}

fn parse_publish(packet_size: u32, flags: u8, input: &[u8]) -> MqttParserResult<&[u8], Publish> {
    let duplicate = flags & 0b0000_1000 == 0b0000_1000;
    let qos = match (flags & 0b0000_0110) >> 1 {
        0u8 => QoS::AtMostOnce,
        1u8 => QoS::AtLeastOnce,
        2u8 => QoS::ExactlyOnce,
        _ => {
            return Err(nom::Err::Failure(MalformedPacket(
                "Only 0|1|2 is valid QoS values.".to_owned(),
            )))
        }
    };
    let retain = (flags & 0b0000_0001) == 0b0000_0001;

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
            // properties
            payload_format_indicator: properties.payload_format_indicator,
            message_expiry_interval: properties.message_expiry_interval,
            topic_alias: properties.topic_alias,
            response_topic: properties.response_topic,
            correlation_data: properties.correlation_data,
            user_properties: properties.user_property,
            subscription_identifier: properties.subscription_identifier,
            content_type: properties.content_type,
            // payload
            payload: payload.to_vec(),
        },
    ))
}

fn parse_subscribe(packet_size: u32, input: &[u8]) -> MqttParserResult<&[u8], Subscribe> {
    println!("parse_subscribe: {}", packet_size);
    let len = input.len();
    let (input, packet_identifier) = u16(Endianness::Big)(input)?;
    let (mut input, properties) = parse_properties(input)?;
    let mut remaining = packet_size - (len - input.len()) as u32;
    let mut topic_filters = vec![];
    while remaining > 0 {
        let len = input.len();
        let (input_tmp, topic_name) = parse_string(input)?;
        let (input_tmp, options) = take(1usize)(input_tmp)?;

        let maximum_qos = match options[0] & 0b0000_0011u8 {
            0u8 => QoS::AtMostOnce,
            1u8 => QoS::AtLeastOnce,
            2u8 => QoS::ExactlyOnce,
            _ => {
                return Err(nom::Err::Failure(MalformedPacket(
                    "Only 0|1|2 is valid QoS values.".to_owned(),
                )))
            }
        };

        let no_local = options[0] & 0b0000_0100u8 == 0b0000_0100;
        let retain_as_published = options[0] & 0b0000_1000u8 == 0b0000_1000;
        let retain_handling = match (options[0] & 0b0011_0000u8) >> 4 {
            0u8 => RetainHandling::SendRetained,
            1u8 => RetainHandling::SendRetainedForNewSubscription,
            2u8 => RetainHandling::DoNotSendRetained,
            _ => {
                return Err(nom::Err::Failure(MalformedPacket(
                    "Only 0|1|2 is valid RetainHandling values.".to_owned(),
                )))
            }
        };

        topic_filters.push(TopicFilter {
            filter: topic_name,
            maximum_qos,
            no_local,
            retain_as_published,
            retain_handling,
        });

        input = input_tmp;
        let advance = (len - input.len()) as u32;
        if advance > remaining {
            return Err(nom::Err::Failure(MalformedPacket(
                "Subscribe parsing: advance > remaining.".to_string(),
            )));
        }

        remaining -= (len - input.len()) as u32;
    }

    Ok((
        input,
        Subscribe {
            packet_identifier,
            subscription_identifier: properties.subscription_identifier,
            user_properties: properties.user_property,
            topic_filters,
        },
    ))
}

fn parse_unsuback(packet_size: u32, rest: &[u8]) -> MqttParserResult<&[u8], UnsubAck> {
    let len = rest.len();
    let (rest, packet_identifier) = u16(Endianness::Big)(rest)?;
    let (rest, properties) = parse_properties(rest)?;

    let variable_header_len = len - rest.len();
    let remaining_len = packet_size - (variable_header_len as u32);
    if remaining_len == 0 {
        return Err(nom::Err::Failure(MalformedPacket(
            "parsing unsuback: must have payload".to_string(),
        )));
    }

    let (rest, payload) = take(remaining_len)(rest)?;
    let reasons = payload
        .iter()
        .map(|i| match i {
            0u8 => Ok(UnsubscribeReason::Success),
            17u8 => Ok(UnsubscribeReason::NoSubscriptionExisted),
            128u8 => Ok(UnsubscribeReason::UnspecifiedError),
            131u8 => Ok(UnsubscribeReason::ImplementationSpecificError),
            135u8 => Ok(UnsubscribeReason::NotAuthorized),
            143u8 => Ok(UnsubscribeReason::TopicFilterInvalid),
            145u8 => Ok(UnsubscribeReason::PacketIdentifierInUse),
            _ => Err(nom::Err::Failure(MalformedPacket(
                "Failed to parse unsubscribe reason".to_string(),
            ))),
        })
        .collect::<Result<Vec<UnsubscribeReason>, _>>()?;

    Ok((
        rest,
        UnsubAck {
            packet_identifier,
            reason_string: properties.reason_string.to_owned(),
            user_properties: properties.user_property,
            reasons,
        },
    ))
}

fn parse_unsubscribe(packet_size: u32, input: &[u8]) -> MqttParserResult<&[u8], Unsubscribe> {
    let len = input.len();
    let (rest, packet_identifier) = u16(Endianness::Big)(input)?;
    let (mut rest, properties) = parse_properties(rest)?;

    let variable_header_len = len - rest.len();
    let mut remaining_len = packet_size - (variable_header_len as u32);
    if remaining_len == 0 {
        return Err(nom::Err::Failure(MalformedPacket(
            "parse unsubscribe: must have payload".to_string(),
        )));
    }

    let mut topic_filters = Vec::new();
    while remaining_len > 0 {
        let (temp_rest, topic_filter) = parse_string(rest)?;
        topic_filters.push(topic_filter);
        remaining_len -= (rest.len() - temp_rest.len()) as u32;
        rest = temp_rest;
    }

    Ok((
        rest,
        Unsubscribe {
            packet_identifier,
            user_properties: properties.user_property,
            topic_filters,
        },
    ))
}

fn parse_suback(packet_size: u32, rest: &[u8]) -> MqttParserResult<&[u8], SubAck> {
    let rest_len = rest.len();
    let (rest, packet_identifier) = u16(Endianness::Big)(rest)?;
    let (rest, properties) = parse_properties(rest)?;

    let remaining_len = packet_size - ((rest_len - rest.len()) as u32);

    let (rest, remaining) = take(remaining_len)(rest)?;

    let reasons: Vec<SubscribeReason> = remaining
        .iter()
        .map(|i| match i {
            0u8 => Ok(SubscribeReason::GrantedQoS0),
            _ => Err(nom::Err::Failure(MalformedPacket(
                "Failed to parse SubscribeReason.".to_string(),
            ))),
        })
        .collect::<Result<Vec<SubscribeReason>, _>>()?;

    Ok((
        rest,
        SubAck {
            packet_identifier,
            reason_string: properties.reason_string,
            user_properties: properties.user_property,
            reasons,
        },
    ))
}

fn parse_connack(input: &[u8]) -> MqttParserResult<&[u8], ConnAck> {
    let (input, connect_acknowledge_flags) = take(1usize)(input)?;
    let session_present = connect_acknowledge_flags[0] & 0b0000_0001 == 1;
    let (input, connect_reason_byte) = take(1usize)(input)?;
    let connect_reason = match connect_reason_byte[0] {
        0 => ConnectReason::Success,
        128 => ConnectReason::UnspecifiedError,
        129 => ConnectReason::MalformedPacket,
        130 => ConnectReason::ProtocolError,
        131 => ConnectReason::ImplementationSpecificError,
        132 => ConnectReason::UnsupportedProtocolVersion,
        133 => ConnectReason::ClientIdentifierNotValid,
        134 => ConnectReason::BadUserNameOrPassword,
        135 => ConnectReason::NotAuthorized,
        136 => ConnectReason::ServerUnavailable,
        137 => ConnectReason::ServerBusy,
        138 => ConnectReason::Banned,
        140 => ConnectReason::BadAuthenticationMethod,
        144 => ConnectReason::TopicNameInvalid,
        149 => ConnectReason::PacketTooLarge,
        151 => ConnectReason::QuotaExceeded,
        153 => ConnectReason::PayloadFormatInvalid,
        154 => ConnectReason::RetainNotSupported,
        155 => ConnectReason::QoSNotSupported,
        156 => ConnectReason::UseAnotherServer,
        157 => ConnectReason::ServerMoved,
        159 => ConnectReason::ConnectionRateExceeded,
        _ => {
            return Err(nom::Err::Failure(MalformedPacket(
                "Failed to parse ConnectReason".to_string(),
            )))
        }
    };

    let (input, properties) = parse_properties(input)?;

    Ok((
        input,
        ConnAck {
            session_present,
            connect_reason,
            session_expiry_interval: properties.session_expiry_interval,
            receive_maximum: properties.receive_maximum,
            maximum_qos: properties.maximum_qos,
            retain_available: properties.retain_available,
            maximum_packet_size: properties.maximum_packet_size,
            assigned_client_identifier: properties.assigned_client_identifier,
            topic_alias_maximum: properties.topic_alias_maximum,
            reason_string: properties.reason_string,
            user_properties: properties.user_property,
        },
    ))
}

pub fn parse_mqtt(input: &[u8]) -> MqttParserResult<&[u8], MqttPacket> {
    let (rest, header) = parse_header(input)?;

    match header.packet_type {
        1u8 => map(parse_connect, MqttPacket::Connect)(rest),
        2u8 => map(parse_connack, MqttPacket::ConnAck)(rest),
        3u8 => parse_publish(header.packet_size, header.flags, rest)
            .map(|(r, publish)| (r, MqttPacket::Publish(publish))),
        8u8 => parse_subscribe(header.packet_size, rest)
            .map(|(r, subscribe)| (r, MqttPacket::Subscribe(subscribe))),
        9u8 => parse_suback(header.packet_size, rest)
            .map(|(r, suback)| (r, MqttPacket::SubAck(suback))),
        10u8 => parse_unsubscribe(header.packet_size, rest)
            .map(|(r, unsubscribe)| (r, MqttPacket::Unsubscribe(unsubscribe))),
        11u8 => parse_unsuback(header.packet_size, rest)
            .map(|(r, unsuback)| (r, MqttPacket::UnsubAck(unsuback))),
        12u8 => {
            if header.flags != 0u8 || header.packet_size != 0u32 {
                Err(nom::Err::Failure(MalformedPacket(
                    "Parsing PingReq: can't have flags or packet size.".to_string(),
                )))
            } else {
                Ok((rest, MqttPacket::PingReq))
            }
        }
        13u8 => {
            if header.flags != 0u8 || header.packet_size != 0u32 {
                Err(nom::Err::Failure(MalformedPacket(
                    "Parsing PingResp: can't have flags or packet size.".to_string(),
                )))
            } else {
                Ok((rest, MqttPacket::PingResp))
            }
        }
        14u8 => map(parse_disconnect, MqttPacket::Disconnect)(rest),
        unknown_packet_type => Err(nom::Err::Failure(MalformedPacket(format!(
            "Unknown packet type: {}.",
            unknown_packet_type
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_string, parse_variable_u32};

    macro_rules! variable_uint_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() -> Result<(), String> {
                let (input, expected) = $value;
                if let Ok((rest, value)) = parse_variable_u32(input) {
                    assert_eq!(value, expected);
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
        one_lower: (&[0b0000_0000u8], 0u32),
        one_upper: (&[0x7fu8], 127u32),
        two_lower: (&[0x80u8, 0x01u8], 128u32),
        two_upper: (&[0xffu8, 0x7fu8], 16_383u32),
        three_lower: (&[0x80u8, 0x80u8, 0x01u8], 16_384u32),
        three_upper: (&[0xffu8, 0xffu8, 0x7fu8], 2_097_151u32),
        four_lower: (&[0x80u8, 0x80u8, 0x80u8, 0x01u8], 2_097_152u32),
        four_upper: (&[0xffu8, 0xffu8, 0xffu8, 0x07fu8], 268_435_455u32),
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
