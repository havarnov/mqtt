use crate::types::{
    ConnAck, Connect, ConnectReason, Disconnect, DisconnectReason, MqttPacket, Properties, Publish,
    QoS, RetainHandling, SubAck, Subscribe, SubscribeReason, UnsubAck, Unsubscribe,
    UnsubscribeReason,
};

fn encode_connect_reason(reason: &ConnectReason) -> u8 {
    match reason {
        ConnectReason::Success => 0,
        ConnectReason::UnspecifiedError => 128,
        ConnectReason::MalformedPacket => 129,
        ConnectReason::ProtocolError => 130,
        ConnectReason::ImplementationSpecificError => 131,
        ConnectReason::UnsupportedProtocolVersion => 132,
        ConnectReason::ClientIdentifierNotValid => 133,
        ConnectReason::BadUserNameOrPassword => 134,
        ConnectReason::NotAuthorized => 135,
        ConnectReason::ServerUnavailable => 136,
        ConnectReason::ServerBusy => 137,
        ConnectReason::Banned => 138,
        ConnectReason::BadAuthenticationMethod => 140,
        ConnectReason::TopicNameInvalid => 144,
        ConnectReason::PacketTooLarge => 149,
        ConnectReason::QuotaExceeded => 151,
        ConnectReason::PayloadFormatInvalid => 153,
        ConnectReason::RetainNotSupported => 154,
        ConnectReason::QoSNotSupported => 155,
        ConnectReason::UseAnotherServer => 156,
        ConnectReason::ServerMoved => 157,
        ConnectReason::ConnectionRateExceeded => 159,
    }
}

fn encode_bool(value: bool) -> u8 {
    match value {
        true => 1,
        false => 0,
    }
}

fn encode_variable_u32(mut value: u32) -> Vec<u8> {
    let mut res = vec![];
    loop {
        let mut byte: u8 = (value % 128) as u8;
        value /= 128;
        if value > 0 {
            byte |= 128;
        }
        res.push(byte);

        if value == 0 {
            break;
        }
    }

    res
}

fn encode_string(string: &str) -> Vec<u8> {
    let bytes: Vec<u8> = string.as_bytes().to_vec();
    let mut result = (bytes.len() as u16).to_be_bytes().to_vec();
    result.extend(&bytes);
    result
}

fn encode_binary(binary: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = (binary.len() as u16).to_be_bytes().to_vec();
    result.extend(binary);
    result
}

fn encode_properties(properties: &Properties) -> Vec<u8> {
    let mut payload = vec![];

    // 1
    if let Some(payload_format_indicator) = properties.payload_format_indicator {
        payload.extend(&encode_variable_u32(1));
        payload.push(payload_format_indicator);
    }

    // 2
    if let Some(message_expiry_interval) = properties.message_expiry_interval {
        payload.extend(&encode_variable_u32(2));
        payload.extend(&message_expiry_interval.to_be_bytes());
    }

    // 3
    if let Some(content_type) = &properties.content_type {
        payload.extend(&encode_variable_u32(3));
        payload.extend(&encode_string(content_type));
    }

    // 8
    if let Some(response_topic) = &properties.response_topic {
        payload.extend(&encode_variable_u32(8));
        payload.extend(&encode_string(response_topic));
    }

    // 9
    if let Some(correlation_data) = &properties.correlation_data {
        payload.extend(&encode_variable_u32(9));
        payload.extend(&encode_binary(correlation_data));
    }

    // 11
    if let Some(subscription_identifier) = properties.subscription_identifier {
        payload.extend(&encode_variable_u32(11));
        payload.extend(&encode_variable_u32(subscription_identifier));
    }

    // 17
    if let Some(session_expiry_interval) = properties.session_expiry_interval {
        payload.extend(&encode_variable_u32(17));
        payload.extend(&session_expiry_interval.to_be_bytes());
    }

    // 18
    if let Some(assigned_client_identifier) = &properties.assigned_client_identifier {
        payload.extend(&encode_variable_u32(18));
        payload.extend(&encode_string(assigned_client_identifier));
    }

    // 19
    if let Some(server_keep_alive) = properties.server_keep_alive {
        payload.extend(&encode_variable_u32(19));
        payload.extend(&server_keep_alive.to_be_bytes());
    }

    // 21
    if let Some(authentication_method) = &properties.authentication_method {
        payload.extend(&encode_variable_u32(21));
        payload.extend(&encode_string(authentication_method));
    }

    // 22
    if let Some(authentication_data) = &properties.authentication_data {
        payload.extend(&encode_variable_u32(22));
        payload.extend(&encode_binary(authentication_data));
    }

    // 23
    if let Some(request_problem_information) = properties.request_problem_information {
        payload.extend(&encode_variable_u32(23));
        payload.push(if request_problem_information {
            1u8
        } else {
            0u8
        });
    }

    // 24
    if let Some(will_delay_interval) = properties.will_delay_interval {
        payload.extend(&encode_variable_u32(24));
        payload.extend(&will_delay_interval.to_be_bytes());
    }

    // 25
    if let Some(request_response_information) = properties.request_response_information {
        payload.extend(&encode_variable_u32(25));
        payload.push(if request_response_information {
            1u8
        } else {
            0u8
        });
    }

    // 26
    if let Some(response_information) = &properties.response_information {
        payload.extend(&encode_variable_u32(26));
        payload.extend(&encode_string(response_information));
    }

    // 28
    if let Some(server_reference) = &properties.server_reference {
        payload.extend(&encode_variable_u32(28));
        payload.extend(&encode_string(server_reference));
    }

    // 31
    if let Some(reason_string) = &properties.reason_string {
        payload.extend(&encode_variable_u32(31));
        payload.extend(&encode_string(reason_string));
    }

    // 33
    if let Some(receive_maximum) = properties.receive_maximum {
        payload.extend(&encode_variable_u32(33));
        payload.extend(&receive_maximum.to_be_bytes());
    }

    // 34
    if let Some(topic_alias_maximum) = properties.topic_alias_maximum {
        payload.extend(&encode_variable_u32(34));
        payload.extend(&topic_alias_maximum.to_be_bytes());
    }

    // 35
    if let Some(topic_alias) = properties.topic_alias {
        payload.extend(&encode_variable_u32(35));
        payload.extend(&topic_alias.to_be_bytes());
    }

    // 36
    if let Some(maximum_qos) = properties.maximum_qos.as_ref() {
        payload.extend(&encode_variable_u32(36));
        payload.push(encode_qos(maximum_qos));
    }

    // 37
    if let Some(retain_available) = properties.retain_available {
        payload.extend(&encode_variable_u32(37));
        payload.push(if retain_available { 1u8 } else { 0u8 });
    }

    // 38
    if let Some(user_properties) = &properties.user_property {
        for user_property in user_properties {
            payload.extend(&encode_variable_u32(38));
            payload.extend(&encode_string(&user_property.key));
            payload.extend(&encode_string(&user_property.value));
        }
    }

    // 39
    if let Some(maximum_packet_size) = properties.maximum_packet_size {
        payload.extend(&encode_variable_u32(39));
        payload.extend(&maximum_packet_size.to_be_bytes());
    }

    // 40
    if let Some(wildcard_subscription_available) = properties.wildcard_subscription_available {
        payload.extend(&encode_variable_u32(40));
        payload.push(encode_bool(wildcard_subscription_available));
    }

    // 41
    if let Some(subscription_identifier_available) = properties.subscription_identifiers_available {
        payload.extend(&encode_variable_u32(41));
        payload.push(encode_bool(subscription_identifier_available));
    }

    // 42
    if let Some(shared_subscription_available) = properties.shared_subscription_available {
        payload.extend(&encode_variable_u32(42));
        payload.push(encode_bool(shared_subscription_available));
    }

    let mut res = encode_variable_u32(payload.len() as u32);
    res.extend(&payload);
    res
}

fn encode_qos(qos: &QoS) -> u8 {
    match qos {
        QoS::AtMostOnce => 0,
        QoS::AtLeastOnce => 1,
        QoS::ExactlyOnce => 2,
    }
}

fn encode_unsuback(unsuback: &UnsubAck) -> Vec<u8> {
    let mut variable_header_and_payload = vec![];
    variable_header_and_payload.extend(unsuback.packet_identifier.to_be_bytes());

    let properties = Properties {
        reason_string: unsuback.reason_string.to_owned(),
        user_property: unsuback.user_properties.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    variable_header_and_payload.extend(unsuback.reasons.iter().map(|i| match i {
        UnsubscribeReason::Success => 0u8,
        UnsubscribeReason::NoSubscriptionExisted => 17u8,
        UnsubscribeReason::UnspecifiedError => 128u8,
        UnsubscribeReason::ImplementationSpecificError => 131u8,
        UnsubscribeReason::NotAuthorized => 135u8,
        UnsubscribeReason::TopicFilterInvalid => 143u8,
        UnsubscribeReason::PacketIdentifierInUse => 145u8,
    }));

    let total_len = &encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result = Vec::with_capacity(variable_header_and_payload.len() + total_len.len() + 1);
    result.push(0b1011_0000);
    result.extend(total_len);
    result.extend(&variable_header_and_payload);

    result
}

fn encode_disconnect(disconnect: &Disconnect) -> Vec<u8> {
    let mut variable_header_and_payload = vec![match disconnect.disconnect_reason {
        DisconnectReason::NormalDisconnection => 0u8,
        DisconnectReason::ProtocolError => 130u8,
        DisconnectReason::SessionTakenOver => 142u8,
        DisconnectReason::TopicAliasInvalid => 148u8,
    }];

    let properties = Properties {
        session_expiry_interval: disconnect.session_expiry_interval,
        reason_string: disconnect.reason_string.to_owned(),
        user_property: disconnect.user_properties.to_owned(),
        server_reference: disconnect.server_reference.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    let total_len = &encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result = Vec::with_capacity(variable_header_and_payload.len() + total_len.len() + 1);
    result.push(0b1110_0000);
    result.extend(total_len);
    result.extend(&variable_header_and_payload);

    result
}

fn encode_connack(connack: &ConnAck) -> Vec<u8> {
    let properties = Properties {
        session_expiry_interval: connack.session_expiry_interval,
        receive_maximum: connack.receive_maximum,
        maximum_qos: connack.maximum_qos.clone(),
        retain_available: connack.retain_available,
        maximum_packet_size: connack.maximum_packet_size,
        assigned_client_identifier: connack.assigned_client_identifier.to_owned(),
        topic_alias_maximum: connack.topic_alias_maximum,
        reason_string: connack.reason_string.to_owned(),
        user_property: connack.user_properties.clone(),
        ..Default::default()
    };

    let mut variable_header = vec![
        if connack.session_present {
            0b0000_0001u8
        } else {
            0u8
        },
        encode_connect_reason(&connack.connect_reason),
    ];
    variable_header.extend(&encode_properties(&properties));

    let mut result = vec![0b0010_0000u8];
    result.extend(&encode_variable_u32(variable_header.len() as u32));
    result.extend(&variable_header);
    result
}

fn encode_subscribe(subscribe: &Subscribe) -> Vec<u8> {
    let mut variable_header_and_payload = vec![];
    variable_header_and_payload.extend(subscribe.packet_identifier.to_be_bytes());
    let properties = Properties {
        subscription_identifier: subscribe.subscription_identifier,
        user_property: subscribe.user_properties.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    for topic_filter in subscribe.topic_filters.iter() {
        variable_header_and_payload.extend(&encode_string(&topic_filter.filter));
        let mut options_byte = 0u8;
        let retain_handling_u8 = match topic_filter.retain_handling {
            RetainHandling::SendRetained => 0u8,
            RetainHandling::SendRetainedForNewSubscription => 1u8,
            RetainHandling::DoNotSendRetained => 2u8,
        };
        options_byte |= retain_handling_u8 << 4;
        options_byte |= if topic_filter.retain_as_published {
            0b0000_1000u8
        } else {
            0u8
        };
        options_byte |= if topic_filter.no_local {
            0b0000_0100u8
        } else {
            0u8
        };
        options_byte |= match topic_filter.maximum_qos {
            QoS::AtMostOnce => 0u8,
            QoS::AtLeastOnce => 1u8,
            QoS::ExactlyOnce => 2u8,
        };
        variable_header_and_payload.push(options_byte);
    }

    let total_len = &encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result = Vec::with_capacity(variable_header_and_payload.len() + total_len.len());
    result.push(0b1000_0010);
    result.extend(total_len);
    result.extend(&variable_header_and_payload);

    result
}

fn encode_unsubscribe(unsubscribe: &Unsubscribe) -> Vec<u8> {
    let mut variable_header_and_payload = vec![];
    variable_header_and_payload.extend(unsubscribe.packet_identifier.to_be_bytes());
    let properties = Properties {
        user_property: unsubscribe.user_properties.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    for topic_filter in unsubscribe.topic_filters.iter() {
        variable_header_and_payload.extend(&encode_string(topic_filter));
    }

    let total_len = &encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result = Vec::with_capacity(variable_header_and_payload.len() + total_len.len());
    result.push(0b1010_0000);
    result.extend(total_len);
    result.extend(&variable_header_and_payload);

    result
}

fn encode_suback(suback: &SubAck) -> Vec<u8> {
    let mut variable_header_and_payload = Vec::new();
    variable_header_and_payload.extend(suback.packet_identifier.to_be_bytes());

    let properties = Properties {
        reason_string: suback.reason_string.to_owned(),
        user_property: suback.user_properties.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    variable_header_and_payload.extend(suback.reasons.iter().map(|i| match i {
        SubscribeReason::GrantedQoS0 => 0u8,
        SubscribeReason::UnspecifiedError => 128u8,
    }));

    let total_len = &encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result = Vec::with_capacity(variable_header_and_payload.len() + total_len.len() + 1);
    result.push(0b1001_0000);
    result.extend(total_len);
    result.extend(&variable_header_and_payload);

    result
}

fn encode_publish(publish: &Publish) -> Vec<u8> {
    let mut first_byte = 0b0011_0000u8;

    if publish.duplicate {
        first_byte |= 0b0000_1000u8;
    }

    first_byte |= match publish.qos {
        QoS::AtMostOnce => 0u8,
        QoS::AtLeastOnce => 1u8 << 1,
        QoS::ExactlyOnce => 2u8 << 1,
    };

    if publish.retain {
        first_byte |= 0b0000_0001u8;
    }

    let mut variable_header_and_payload = Vec::new();
    variable_header_and_payload.extend(encode_string(&publish.topic_name));
    if let Some(packet_identifier) = publish.packet_identifier {
        variable_header_and_payload.extend(packet_identifier.to_be_bytes());
    }

    let properties = Properties {
        payload_format_indicator: publish.payload_format_indicator,
        message_expiry_interval: publish.message_expiry_interval,
        topic_alias: publish.topic_alias,
        response_topic: publish.response_topic.to_owned(),
        correlation_data: publish.correlation_data.to_owned(),
        user_property: publish.user_properties.to_owned(),
        subscription_identifier: publish.subscription_identifier,
        content_type: publish.content_type.to_owned(),
        ..Default::default()
    };
    variable_header_and_payload.extend(&encode_properties(&properties));

    variable_header_and_payload.extend(&publish.payload);

    let packet_length = encode_variable_u32(variable_header_and_payload.len() as u32);
    let mut result =
        Vec::with_capacity(variable_header_and_payload.len() + packet_length.len() + 1);
    result.push(first_byte);
    result.extend(packet_length);
    result.extend(variable_header_and_payload);
    result
}

fn encode_connect(connect: &Connect) -> Vec<u8> {
    let mut connect_flags = 0u8;

    if connect.username.is_some() {
        connect_flags |= 0b1000_0000u8;
    }

    if connect.password.is_some() {
        connect_flags |= 0b0100_0000u8;
    }

    if let Some(will) = &connect.will {
        connect_flags |= 0b0000_0100u8;

        if will.retain {
            connect_flags |= 0b0010_0000u8;
        }

        match will.qos {
            1u8 => connect_flags |= 0b0000_1000u8,
            2u8 => connect_flags |= 0b0001_0000u8,
            0u8 => (),
            _ => unreachable!(),
        }
    }

    if connect.clean_start {
        connect_flags |= 0b0000_0010u8;
    }

    let mut variable_header = vec![
        // Protocol name (MQTT)
        0u8,
        4u8,
        0b01001101u8,
        0b01010001u8,
        0b01010100u8,
        0b01010100u8,
        // Protocol version (5)
        5u8,
        // connect flags
        connect_flags,
    ];

    // keep alive
    variable_header.extend(&connect.keep_alive.to_be_bytes());

    let properties = Properties {
        session_expiry_interval: connect.session_expiry_interval,
        receive_maximum: connect.receive_maximum,
        maximum_packet_size: connect.maximum_packet_size,
        topic_alias_maximum: connect.topic_alias_maximum,
        request_response_information: connect.request_response_information,
        request_problem_information: connect.request_problem_information,
        user_property: connect.user_properties.to_owned(),
        ..Default::default()
    };
    variable_header.extend(&encode_properties(&properties));

    let mut payload: Vec<u8> = vec![];
    payload.extend(&encode_string(&connect.client_identifier));

    if let Some(will) = &connect.will {
        println!("{:?}", will);

        let will_properties = Properties {
            will_delay_interval: will.delay_interval,
            payload_format_indicator: will.payload_format_indicator,
            message_expiry_interval: will.message_expiry_interval,
            content_type: will.content_type.to_owned(),
            response_topic: will.response_topic.to_owned(),
            correlation_data: will.correlation_data.to_owned(),
            user_property: will.user_properties.to_owned(),
            ..Default::default()
        };
        payload.extend(&encode_properties(&will_properties));

        payload.extend(&encode_string(&will.topic));
        payload.extend(&encode_binary(&will.payload));
    }

    if let Some(username) = &connect.username {
        payload.extend(&encode_string(username));
    }

    if let Some(password) = &connect.password {
        payload.extend(&encode_string(password));
    }

    let mut result = vec![0b0001_0000u8];
    result.extend(&encode_variable_u32(
        (variable_header.len() + payload.len()) as u32,
    ));
    result.extend(&variable_header);
    result.extend(&payload);
    result
}

pub fn encode(packet: &MqttPacket) -> Vec<u8> {
    match packet {
        MqttPacket::PingResp => {
            return vec![0b1101_0000u8, 0u8];
        }
        MqttPacket::PingReq => {
            return vec![0b1100_0000u8, 0u8];
        }
        MqttPacket::ConnAck(c) => encode_connack(c),
        MqttPacket::Publish(publish) => encode_publish(publish),
        MqttPacket::Subscribe(subscribe) => encode_subscribe(subscribe),
        MqttPacket::SubAck(suback) => encode_suback(suback),
        MqttPacket::Connect(connect) => encode_connect(connect),
        MqttPacket::Unsubscribe(unsubscribe) => encode_unsubscribe(unsubscribe),
        MqttPacket::UnsubAck(unsuback) => encode_unsuback(unsuback),
        MqttPacket::Disconnect(disconnect) => encode_disconnect(disconnect),
        _ => unimplemented!("to_bytes"),
    }
}

#[cfg(test)]
mod tests {
    use super::encode_variable_u32;

    macro_rules! variable_uint_tests {
    ($($name:ident: $value:expr,)*) => {
    $(
        #[test]
        fn $name() {
            let (expected, input) = $value;
            assert_eq!(&expected[..], &encode_variable_u32(input)[..]);
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
}
