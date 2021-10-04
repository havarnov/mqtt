#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    Connect(Connect),
    ConnAck(ConnAck),
    Publish(Publish),
    PubAck,  // TODO: impl decode/encode
    PubRec,  // TODO: impl decode/encode
    PubRel,  // TODO: impl decode/encode
    PubComp, // TODO: impl decode/encode
    Subscribe(Subscribe),
    SubAck(SubAck),
    Unsubscribe(Unsubscribe),
    UnsubAck, // TODO: impl decode/encode
    PingReq,
    PingResp,
    Disconnect(Disconnect),
    Auth, // TODO: impl decode/encode
}

#[derive(Debug, PartialEq, Default)]
pub struct Properties {
    // 1
    pub payload_format_indicator: Option<u8>,
    // 2
    pub message_expiry_interval: Option<u32>,
    // 3
    pub content_type: Option<String>,
    // 8
    pub response_topic: Option<String>,
    // 9
    pub correlation_data: Option<Vec<u8>>,
    // 11
    pub subscription_identifier: Option<u32>,
    // 17
    pub session_expiry_interval: Option<u32>,
    // 18
    pub assigned_client_identifier: Option<String>,
    // 19
    pub server_keep_alive: Option<u16>,
    // 21
    pub authentication_method: Option<String>,
    // 22
    pub authentication_data: Option<Vec<u8>>,
    // 23
    pub request_problem_information: Option<bool>,
    // 24
    pub will_delay_interval: Option<u32>,
    // 25
    pub request_response_information: Option<bool>,
    // 26
    pub response_information: Option<String>,
    // 28
    pub server_reference: Option<String>,
    // 31
    pub reason_string: Option<String>,
    // 33
    pub receive_maximum: Option<u16>,
    // 34
    pub topic_alias_maximum: Option<u16>,
    // 35
    pub topic_alias: Option<u16>,
    // 36
    pub maximum_qos: Option<u8>,
    // 37
    pub retain_available: Option<bool>,
    // 38
    pub user_property: Option<Vec<UserProperty>>,
    // 39
    pub maximum_packet_size: Option<u32>,
    // 40
    pub wildcard_subscription_available: Option<u8>,
    // 41
    pub subscription_identifier_available: Option<u8>,
    // 42
    pub shared_subscription_available: Option<u8>,
}

impl Properties {
    pub fn new() -> Properties {
        Default::default()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct UserProperty {
    pub key: String,
    pub value: String,
}

#[derive(Debug, PartialEq)]
pub struct Will {
    pub retain: bool,
    pub qos: u8,
    pub topic: String,
    pub payload: Vec<u8>,
    // properties
    pub delay_interval: Option<u32>,
    pub payload_format_indicator: Option<u8>,
    pub message_expiry_interval: Option<u32>,
    pub content_type: Option<String>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Option<Vec<UserProperty>>,
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
    pub session_expiry_interval: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub maximum_packet_size: Option<u32>,
    pub topic_alias_maximum: Option<u16>,
    pub request_response_information: Option<bool>,
    pub request_problem_information: Option<bool>,
    pub user_properties: Option<Vec<UserProperty>>,
    pub authentication_method: Option<String>,
    pub authentication_data: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub enum ConnectReason {
    // 0 The Connection is accepted.
    Success,
    // 128 The Server does not wish to reveal the reason for the failure, or none of the other Reason Codes apply.
    UnspecifiedError,
    // 129 Data within the CONNECT packet could not be correctly parsed.
    MalformedPacket,
    // 130 Data in the CONNECT packet does not conform to this specification.
    ProtocolError,
    // 131 The CONNECT is valid but is not accepted by this Server.
    ImplementationSpecificError,
    // 132 The Server does not support the version of the MQTT protocol requested by the Client.
    UnsupportedProtocolVersion,
    // 133 The Client Identifier is a valid string but is not allowed by the Server.
    ClientIdentifierNotValid,
    // 134 The Server does not accept the User Name or Password specified by the Client
    BadUserNameOrPassword,
    // 135 The Client is not authorized to connect.
    NotAuthorized,
    // 136 The MQTT Server is not available.
    ServerUnavailable,
    // 137 The Server is busy. Try again later.
    ServerBusy,
    // 138 This Client has been banned by administrative action. Contact the server administrator.
    Banned,
    // 140 The authentication method is not supported or does not match the authentication method currently in use.
    BadAuthenticationMethod,
    // 144 The Will Topic Name is not malformed, but is not accepted by this Server.
    TopicNameInvalid,
    // 149 The CONNECT packet exceeded the maximum permissible size.
    PacketTooLarge,
    // 151 An implementation or administrative imposed limit has been exceeded.
    QuotaExceeded,
    // 153 The Will Payload does not match the specified Payload Format Indicator.
    PayloadFormatInvalid,
    // 154 The Server does not support retained messages, and Will Retain was set to 1.
    RetainNotSupported,
    // 155 The Server does not support the QoS set in Will QoS.
    QoSNotSupported,
    // 156 The Client should temporarily use another server.
    UseAnotherServer,
    // 157 The Client should permanently use another server.
    ServerMoved,
    // 159 The connection rate limit has been exceeded.
    ConnectionRateExceeded,
}

#[derive(Debug, PartialEq)]
pub struct ConnAck {
    pub session_present: bool,
    pub connect_reason: ConnectReason,
    // properties
    pub session_expiry_interval: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub maximum_qos: Option<u8>,
    pub retain_available: Option<bool>,
    pub maximum_packet_size: Option<u32>,
    pub assigned_client_identifier: Option<String>,
    pub topic_alias_maximum: Option<u16>,
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<UserProperty>>,
}

#[derive(Debug, PartialEq)]
pub enum DisconnectReason {
    NormalDisconnection,
    // TODO...
}

#[derive(Debug, PartialEq)]
pub struct Disconnect {
    pub disconnect_reason: DisconnectReason,
    pub properties: Properties,
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
    // properties
    pub payload_format_indicator: Option<u8>,
    pub message_expiry_interval: Option<u32>,
    pub topic_alias: Option<u16>,
    pub response_topic: Option<String>,
    pub correlation_data: Option<Vec<u8>>,
    pub user_properties: Option<Vec<UserProperty>>,
    pub subscription_identifier: Option<u32>,
    pub content_type: Option<String>,
    // actual payload
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
    pub subscription_identifier: Option<u32>,
    pub user_properties: Option<Vec<UserProperty>>,
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
    pub properties: Properties,
    pub reasons: Vec<SubscribeReason>,
}

#[derive(Debug, PartialEq)]
pub struct Unsubscribe {
    pub packet_identifier: u16,
    pub topic_filters: Vec<String>,
    // properties
    pub user_properties: Option<Vec<UserProperty>>,
}
