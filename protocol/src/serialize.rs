use crate::types::{ConnectReason, MqttPacket, QoS, SubscribeReason};

pub fn to_bytes(packet: &MqttPacket) -> Vec<u8> {
    match packet {
        MqttPacket::PingResp => {
            return vec![0b1101_0000u8, 0u8];
        }
        MqttPacket::ConnAck(c) => {
            let mut variable_header = vec![];

            variable_header.push(if c.session_present { 0b0000_0001 } else { 0u8 });

            match c.connect_reason {
                ConnectReason::Success => variable_header.push(0u8),
            }

            // TODO: properties
            // empty properties
            variable_header.push(0u8);

            let mut fixed_header = vec![0b0010_0000u8, variable_header.len() as u8];
            fixed_header.extend_from_slice(&variable_header);

            return fixed_header;
        }
        MqttPacket::Publish(publish) => {
            let mut variable_header_and_payload = vec![];

            let topic_name = publish.topic_name.as_bytes();
            let topic_name_len = topic_name.len() as u8;
            variable_header_and_payload.push(0u8);
            variable_header_and_payload.push(topic_name_len);
            variable_header_and_payload.extend_from_slice(topic_name);

            if let Some(packet_identifier) = publish.packet_identifier {
                variable_header_and_payload.push(0u8);
                variable_header_and_payload.push(packet_identifier as u8);
            }

            // TODO: properties
            // empty properties
            variable_header_and_payload.push(0u8);

            // payload
            variable_header_and_payload.extend_from_slice(&publish.payload);

            let mut fst = 0b0011_0000u8;
            if publish.duplicate {
                fst = fst | 0b0000_1000u8;
            }

            fst = fst
                | match publish.qos {
                    QoS::AtMostOnce => 0u8,
                    QoS::AtLeastOnce => 1u8,
                    QoS::ExactlyOnce => 2u8,
                };

            if publish.retain {
                fst = fst | 0b0000_0001u8;
            }

            let mut result = vec![fst, variable_header_and_payload.len() as u8];
            result.extend_from_slice(&variable_header_and_payload);
            return result;
        }
        MqttPacket::SubAck(sub_ack) => {
            let mut variable_header_and_payload = vec![];
            // TODO encode i16
            let pi = sub_ack.packet_identifier as u8;
            variable_header_and_payload.push(pi);

            // TODO: properties
            // empty properties
            variable_header_and_payload.push(0u8);

            for reason in sub_ack.reasons.iter() {
                variable_header_and_payload.push(match reason {
                    SubscribeReason::GrantedQoS0 => 0u8,
                });
            }

            let mut fixed_header = vec![0b1001_0000, variable_header_and_payload.len() as u8];
            fixed_header.extend_from_slice(&variable_header_and_payload);
            return fixed_header;
        }
        _ => unimplemented!("to_bytes"),
    }
}
