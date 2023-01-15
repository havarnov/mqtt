#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    Connect(Connect),
    ConnAck(ConnAck),
    Publish(Publish),
    PubAck(PubAck),
    PubRec,  // TODO: impl decode/encode
    PubRel,  // TODO: impl decode/encode
    PubComp, // TODO: impl decode/encode
    Subscribe(Subscribe),
    SubAck(SubAck),
    Unsubscribe(Unsubscribe),
    UnsubAck(UnsubAck),
    PingReq,
    PingResp,
    Disconnect(Disconnect),
    Auth, // TODO: impl decode/encode
}

/// All possible MQTT properties
#[derive(Debug, PartialEq, Default)]
pub struct Properties {
    /// Payload Format Indicator (0x01) - Byte
    pub payload_format_indicator: Option<u8>,

    /// Message Expiry Interval (0x02) - Four Byte Integer
    pub message_expiry_interval: Option<u32>,

    /// Content Type (0x03) - UTF-8 Encoded String
    pub content_type: Option<String>,

    /// Response Topic (0x08) - UTF-8 Encoded String
    pub response_topic: Option<String>,

    /// Correlation Data (0x09) - Binary Data
    pub correlation_data: Option<Vec<u8>>,

    /// Subscription Identifier (0x0B) - Variable Byte Integer
    pub subscription_identifier: Option<u32>,

    /// Session Expiry Interval (0x11) - Four Byte Integer
    pub session_expiry_interval: Option<u32>,

    /// Assigned Client Identifier (0x12) - UTF-8 Encoded String
    pub assigned_client_identifier: Option<String>,

    /// Server Keep Alive (0x13) - Two Byte Integer
    pub server_keep_alive: Option<u16>,

    /// Authentication Method (0x15) - UTF-8 Encoded String
    pub authentication_method: Option<String>,

    /// Authentication Data (0x16) - Binary Data
    pub authentication_data: Option<Vec<u8>>,

    /// Request Problem Information (0x17) - Byte
    pub request_problem_information: Option<bool>,

    /// Will Delay Interval (0x18) - Four Byte Integer
    pub will_delay_interval: Option<u32>,

    /// Request Response Information (0x19) - Byte
    pub request_response_information: Option<bool>,

    /// Response Information (0x1A) - UTF-8 Encoded String
    pub response_information: Option<String>,

    /// Server Reference (0x1C) - UTF-8 Encoded String
    pub server_reference: Option<String>,

    /// Reason String (0x1F) - UTF-8 Encoded String
    pub reason_string: Option<String>,

    /// Receive Maximum (0x21) - Two Byte Integer
    pub receive_maximum: Option<u16>,

    /// Topic Alias Maximum (0x22) - Two Byte Integer
    pub topic_alias_maximum: Option<u16>,

    /// Topic Alias (0x23) - Two Byte Integer
    pub topic_alias: Option<u16>,

    /// Maximum QoS (0x24) - Byte
    pub maximum_qos: Option<QoS>,

    /// Retain Available (0x25) - Byte
    pub retain_available: Option<bool>,

    /// User Property (0x26) - UTF-8 String Pair
    pub user_properties: Option<Vec<UserProperty>>,

    /// Maximum Packet Size (0x27) - Four Byte Integer
    pub maximum_packet_size: Option<u32>,

    /// Wildcard Subscription Available (0x28) - Byte
    pub wildcard_subscription_available: Option<bool>,

    /// Subscription Identifier Available (0x29) - Byte
    pub subscription_identifiers_available: Option<bool>,

    /// Shared Subscription Available (0x2A) - Byte
    pub shared_subscription_available: Option<bool>,
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
    pub qos: QoS,
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
    /// 3.2.2.1.1 Session Present
    ///
    /// The Session Present flag informs the Client whether the Server is using Session State from a previous connection for this ClientID.
    pub session_present: bool,

    /// 3.2.2.2 Connect Reason Code
    ///
    /// If a Server sends a CONNACK packet containing a Reason code of 128 or greater it MUST then close the Network Connection.
    pub connect_reason: ConnectReason,

    /// 3.2.2.3.2 Session Expiry Interval
    ///
    /// Representing the Session Expiry Interval in seconds.
    /// TODO: Change to Duration?
    pub session_expiry_interval: Option<u32>,

    /// 3.2.2.3.3 Receive Maximum
    ///
    /// The Server uses this value to limit the number of QoS 1 and QoS 2 publications that it is willing to process concurrently for the Client.
    /// If the Receive Maximum value is absent, then its value defaults to 65,535.
    pub receive_maximum: Option<u16>,

    /// 3.2.2.3.4 Maximum QoS
    ///
    /// If a Server does not support QoS 1 or QoS 2 PUBLISH packets it MUST send a Maximum QoS in the CONNACK packet specifying the highest QoS it supports.
    pub maximum_qos: Option<QoS>,

    /// 3.2.2.3.5 Retain Available
    ///
    /// Declares whether the Server supports retained messages.
    /// If not present, then retained messages are supported.
    pub retain_available: Option<bool>,

    /// 3.2.2.3.6 Maximum Packet Size
    ///
    /// Representing the Maximum Packet Size the Server is willing to accept.
    /// If the Maximum Packet Size is not present, there is no limit on the packet size imposed beyond the
    /// limitations in the protocol as a result of the remaining length encoding and the protocol header sizes.
    pub maximum_packet_size: Option<u32>,

    /// 3.2.2.3.7 Assigned Client Identifier
    ///
    /// The Client Identifier which was assigned by the Server because a zero length Client Identifier was found in the CONNECT packet.
    pub assigned_client_identifier: Option<String>,

    /// 3.2.2.3.8 Topic Alias Maximum
    ///
    /// This value indicates the highest value that the Server will accept as a Topic Alias sent by the Client.
    /// The Server uses this value to limit the number of Topic Aliases that it is willing to hold on this Connection.
    pub topic_alias_maximum: Option<u16>,

    /// 3.2.2.3.9 Reason String
    ///
    /// The Server uses this value to give additional information to the Client.
    /// TODO: The Server MUST NOT send this property if it would increase the size of the CONNACK packet beyond the Maximum Packet Size specified by the Client
    pub reason_string: Option<String>,

    /// 3.2.2.3.10 User Property
    ///
    /// The content and meaning of this property is not defined by this specification.
    /// The receiver of a CONNACK containing this property MAY ignore it.
    pub user_properties: Option<Vec<UserProperty>>,

    /// 3.2.2.3.11 Wildcard Subscription Available
    ///
    /// If present, this declares whether the Server supports Wildcard Subscriptions.
    /// If not present, then Wildcard Subscriptions are supported.
    pub wildcard_subscription_available: Option<bool>,

    /// 3.2.2.3.12 Subscription Identifiers Available
    ///
    /// If present, this byte declares whether the Server supports Subscription Identifiers.
    /// If not present, then Subscription Identifiers are supported.
    pub subscription_identifiers_available: Option<bool>,

    /// 3.2.2.3.13 Shared Subscription Available
    ///
    /// If present, this declares whether the Server supports Shared Subscriptions.
    /// If not present, then Shared Subscriptions are supported.
    pub shared_subscription_available: Option<bool>,

    /// 3.2.2.3.14 Server Keep Alive
    ///
    /// If the Server sends a Server Keep Alive on the CONNACK packet, the Client MUST use this value instead of the Keep Alive value the Client sent on CONNECT.
    /// If the Server does not send the Server Keep Alive, the Server MUST use the Keep Alive value set by the Client on CONNECT.
    pub server_keep_alive: Option<u16>,

    /// 3.2.2.3.15 Response Information
    ///
    /// Used as the basis for creating a Response Topic.
    pub response_information: Option<String>,

    /// 3.2.2.3.16 Server Reference
    ///
    /// Can be used by the Client to identify another Server to use.
    pub server_reference: Option<String>,

    /// 3.2.2.3.17 Authentication Method
    ///
    /// Containing the name of the authentication method.
    pub authentication_method: Option<String>,

    /// 3.2.2.3.18 Authentication Data
    ///
    /// Containing the authentication data.
    /// The contents of this data are defined by the authentication method and the state of already exchanged authentication data.
    pub authentication_data: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq)]
pub enum DisconnectReason {
    NormalDisconnection,
    ProtocolError,
    TopicAliasInvalid,
    SessionTakenOver, // TODO...
}

#[derive(Debug, PartialEq)]
pub struct Disconnect {
    pub disconnect_reason: DisconnectReason,
    // properties
    pub session_expiry_interval: Option<u32>,
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<UserProperty>>,
    pub server_reference: Option<String>,
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

/// [3.3 PUBLISH – Publish message](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901100)
///
/// A PUBLISH packet is sent from a Client to a Server or from a Server to a Client to transport an Application Message.
#[derive(Debug, PartialEq, Clone)]
pub struct Publish {
    /// [3.3.1.1 DUP](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901102)
    ///
    /// If the DUP flag is set to 0, it indicates that this is the first occasion that the Client or
    /// Server has attempted to send this PUBLISH packet. If the DUP flag is set to 1, it indicates
    /// that this might be re-delivery of an earlier attempt to send the packet.
    pub duplicate: bool,

    /// [3.3.1.2 QoS](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901103)
    ///
    /// This field indicates the level of assurance for delivery of an Application Message.
    pub qos: QoS,

    /// [3.3.1.3 RETAIN](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901104)
    ///
    /// If the RETAIN flag is set to 1 in a PUBLISH packet sent by a Client to a Server, the Server
    /// MUST replace any existing retained message for this topic and store the Application Message [MQTT-3.3.1-5],
    /// so that it can be delivered to future subscribers whose subscriptions match its Topic Name.
    pub retain: bool,

    /// [3.3.2.1 Topic Name](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901107)
    ///
    /// The Topic Name identifies the information channel to which Payload data is published.
    pub topic_name: String,

    /// [3.3.2.2 Packet Identifier](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901108)
    ///
    /// The Packet Identifier field is only present in PUBLISH packets where the QoS level is 1 or 2.
    pub packet_identifier: Option<u16>,

    /// [3.3.2.3.2 Payload Format Indicator](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901111)
    ///
    /// (...) the value of the Payload Format Indicator, either of:
    /// * 0 (0x00) Byte Indicates that the Will Message is unspecified bytes, which is equivalent to not sending a Payload Format Indicator.
    /// * 1 (0x01) Byte Indicates that the Will Message is UTF-8 Encoded Character Data. The UTF-8 data in the Payload MUST be well-formed UTF-8 as defined by the Unicode specification [Unicode] and restated in RFC 3629 [RFC3629].
    pub payload_format_indicator: Option<u8>,

    /// [3.3.2.3.3 Message Expiry Interval](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901112)
    ///
    /// (...) is the lifetime of the Application Message in seconds.
    pub message_expiry_interval: Option<u32>,

    /// [3.3.2.3.4 Topic Alias](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901113)
    ///
    /// A Topic Alias is an integer value that is used to identify the Topic instead of using the Topic Name.
    pub topic_alias: Option<u16>,

    /// [3.3.2.3.5 Response Topic](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901114)
    ///
    /// (...) is used as the Topic Name for a response message.
    pub response_topic: Option<String>,

    /// [3.3.2.3.6 Correlation Data](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901115)
    ///
    /// The Correlation Data is used by the sender of the Request Message to identify which request
    /// the Response Message is for when it is received.
    pub correlation_data: Option<Vec<u8>>,

    /// [3.3.2.3.7 User Property](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901116)
    ///
    /// This property is intended to provide a means of transferring application layer name-value
    /// tags whose meaning and interpretation are known only by the application programs responsible
    /// for sending and receiving them.
    pub user_properties: Option<Vec<UserProperty>>,

    /// [3.3.2.3.8 Subscription Identifier](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901117)
    ///
    /// (...) representing the identifier of the subscription.
    pub subscription_identifier: Option<u32>,

    /// [3.3.2.3.9 Content Type](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901118)
    ///
    /// (...) describing the content of the Application Message.
    pub content_type: Option<String>,

    /// [3.3.3 PUBLISH Payload](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901119)
    ///
    /// The Payload contains the Application Message that is being published.
    /// The content and format of the data is application specific.
    pub payload: Vec<u8>,
}

/// [3.4.2.1 PUBACK Reason Code](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901124)
#[derive(Debug, PartialEq)]
pub enum PubAckReason {
    /// 0 - The message is accepted. Publication of the QoS 1 message proceeds.
    Success,

    /// 16 - The message is accepted but there are no subscribers.
    /// This is sent only by the Server. If the Server knows that there are no matching subscribers,
    /// it MAY use this Reason Code instead of 0x00 (Success).
    NoMatchingSubscribers,

    /// 128 - The receiver does not accept the publish but either does not want to reveal the reason,
    /// or it does not match one of the other values.
    UnspecifiedError,

    /// 131 - The PUBLISH is valid but the receiver is not willing to accept it.
    ImplementationSpecificError,

    /// 135 - The PUBLISH is not authorized.
    NotAuthorized,

    /// 144 - The Topic Name is not malformed, but is not accepted by this Client or Server.
    TopicNameInvalid,

    /// 145 - The Packet Identifier is already in use. This might indicate a mismatch in the
    /// Session State between the Client and Server.
    PacketIdentifierInUse,

    /// 151 - An implementation or administrative imposed limit has been exceeded.
    QuotaExceeded,

    /// 153 - The payload format does not match the specified Payload Format Indicator.
    PayloadFormatInvalid,
}

#[derive(Debug, PartialEq)]
pub struct PubAck {
    /// [3.4.2 PUBACK Variable Header](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901123)
    ///
    /// (...) Packet Identifier from the PUBLISH packet that is being acknowledged, (...)
    pub packet_identifier: u16,

    /// [3.4.2.1 PUBACK Reason Code](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901124)
    ///
    /// The Client or Server sending the PUBACK packet MUST use one of the PUBACK Reason Codes (...)
    pub reason: PubAckReason,

    /// [3.4.2.2.2 Reason String](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901127)
    ///
    /// (...) This Reason String is a human readable string designed for diagnostics and is not
    /// intended to be parsed by the receiver.
    ///
    /// The sender uses this value to give additional information to the receiver. The sender MUST NOT
    /// send this property if it would increase the size of the PUBACK packet beyond the
    /// Maximum Packet Size specified by the receiver. It is a Protocol Error to include
    /// the Reason String more than once.
    pub reason_string: Option<String>,

    /// [3.4.2.2.3 User Property](https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html#_Toc3901128)
    ///
    /// (...) This property can be used to provide additional diagnostic or other information. (...)
    pub user_properties: Option<Vec<UserProperty>>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RetainHandling {
    // 0 - Send retained messages at the time of the subscribe
    SendRetained,
    // 1 - Send retained messages at subscribe only if the subscription does not currently exist
    SendRetainedForNewSubscription,
    // 2 - Do not send retained messages at the time of the subscribe
    DoNotSendRetained,
}

#[derive(Debug, PartialEq, Clone)]
pub struct TopicFilter {
    pub filter: String,
    pub maximum_qos: QoS,
    pub no_local: bool,
    pub retain_as_published: bool,
    pub retain_handling: RetainHandling,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Subscribe {
    pub packet_identifier: u16,
    /// The Subscription Identifier is associated with any subscription created or modified as the result of this SUBSCRIBE packet.
    /// If there is a Subscription Identifier, it is stored with the subscription.
    /// If this property is not specified, then the absence of a Subscription Identifier is stored with the subscription.
    pub subscription_identifier: Option<u32>,
    pub user_properties: Option<Vec<UserProperty>>,
    pub topic_filters: Vec<TopicFilter>,
}

#[derive(Debug, PartialEq)]
pub enum UnsubscribeReason {
    Success,
    NoSubscriptionExisted,
    UnspecifiedError,
    ImplementationSpecificError,
    NotAuthorized,
    TopicFilterInvalid,
    PacketIdentifierInUse,
}

#[derive(Debug, PartialEq)]
pub struct UnsubAck {
    pub packet_identifier: u16,
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<UserProperty>>,
    pub reasons: Vec<UnsubscribeReason>,
}

#[derive(Debug, PartialEq)]
pub enum SubscribeReason {
    /// 0
    GrantedQoS0,

    /// 128
    UnspecifiedError,
    // TODO ..
}

#[derive(Debug, PartialEq)]
pub struct SubAck {
    pub packet_identifier: u16,
    // properties
    pub reason_string: Option<String>,
    pub user_properties: Option<Vec<UserProperty>>,
    // payload
    pub reasons: Vec<SubscribeReason>,
}

#[derive(Debug, PartialEq)]
pub struct Unsubscribe {
    pub packet_identifier: u16,
    pub topic_filters: Vec<String>,
    // properties
    pub user_properties: Option<Vec<UserProperty>>,
}
