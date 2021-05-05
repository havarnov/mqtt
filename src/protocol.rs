#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    Connect(Connect),
    ConnAck(ConnAck),
    Disconnect(Disconnect),
    Subscribe(Subscribe),
    SubAck(SubAck),
    PingReq,
    PingResp,
    Publish(Publish),
}

#[derive(Debug, PartialEq)]
pub enum Property {
    SessionExpiryInterval(u32),
    AuthenticationMethod(String),
    AuthenticationData(Vec<u8>),
    RequestProblemInformation(u8),
    RequestResponseInformation(u8),
    ReceiveMaximum(u16),
    TopicAliasMaximum(u16),
    UserProperty { key: String, value: String },
    MaximumPacketSize(u32),
}

#[derive(Debug, PartialEq)]
pub struct Will {
    pub retain: bool,
    pub qos: u8,
    pub topic: String,
    pub properties: Vec<Property>,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct Connect {
    pub protocol_name: String,
    pub protocol_version: u8,
    pub client_identifier: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub will: Option<Will>,
    pub clean_start: bool,
    pub keep_alive: u16,
    pub properties: Vec<Property>,
}

#[derive(Debug, PartialEq)]
pub enum ConnectReason {
    Success, // TODO...
}

#[derive(Debug, PartialEq)]
pub struct ConnAck {
    pub session_present: bool,
    pub connect_reason: ConnectReason,
    pub properties: Vec<Property>,
}

#[derive(Debug, PartialEq)]
pub enum DisconnectReason {
    NormalDisconnection,
    // TODO...
}

#[derive(Debug, PartialEq)]
pub struct Disconnect {
    pub disconnect_reason: DisconnectReason,
    pub properties: Vec<Property>,
}

#[derive(Debug, PartialEq)]
pub enum QoS {
    AtMostOnce,
    AtLeastOnce,
    ExactlyOnce,
}

#[derive(Debug, PartialEq)]
pub struct Publish {
    pub duplicate: bool,
    pub qos: QoS,
    pub retain: bool,
    pub topic_name: String,
    pub packet_identifier: Option<u16>,
    pub properties: Vec<Property>,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct TopicFilter {
    pub topic_name: String,
    pub maximum_qos: QoS,
    pub no_local: bool,
    pub retain_as_published: bool,
    // TODO: change to enum?
    pub retain_handling: u8,
}

#[derive(Debug, PartialEq)]
pub struct Subscribe {
    pub packet_identifier: u16,
    pub properties: Vec<Property>,
    pub topic_filters: Vec<TopicFilter>,
}

#[derive(Debug, PartialEq)]
pub enum SubscribeReason {
    GrantedQoS0,
    // TODO ..
}

#[derive(Debug, PartialEq)]
pub struct SubAck {
    pub packet_identifier: u16,
    pub properties: Vec<Property>,
    pub reasons: Vec<SubscribeReason>,
}

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
