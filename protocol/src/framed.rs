use bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::codec::decoding::parse_mqtt;
use crate::codec::encoding::encode;
use crate::types::MqttPacket;
use std::fmt::{Display, Formatter};
use std::io::Error;

#[derive(Debug)]
pub struct MqttPacketDecoder {}

#[derive(Debug)]
pub struct MqttPacketDecoderError {}

impl From<std::io::Error> for MqttPacketDecoderError {
    fn from(_: Error) -> Self {
        MqttPacketDecoderError {}
    }
}

impl Display for MqttPacketDecoderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "MqttPacketDecoderError: unknown.")
    }
}

impl std::error::Error for MqttPacketDecoderError {}

impl Encoder<MqttPacket> for MqttPacketDecoder {
    type Error = MqttPacketDecoderError;

    fn encode(&mut self, item: MqttPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = encode(&item);
        dst.extend_from_slice(&bytes);
        Ok(())
    }
}

impl Decoder for MqttPacketDecoder {
    type Item = MqttPacket;
    type Error = MqttPacketDecoderError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 2 {
            // We need at least 2 bytes to determine the packet type and size.
            return Ok(None);
        }

        println!("incoming: {:?}", src.to_vec());

        match parse_mqtt(src) {
            Ok((rest, packet)) => {
                let bytes_to_advance = src.len() - rest.len();
                src.advance(bytes_to_advance);
                Ok(Some(packet))
            }
            Err(nom::Err::Incomplete(_)) => Ok(None),
            Err(nom::Err::Failure(err)) => {
                println!("Something failed while parsing: {:?}", err);
                Err(MqttPacketDecoderError {})
            }
            other => {
                println!("Something failed while parsing: {:?}", other);
                Err(MqttPacketDecoderError {})
            }
        }
    }
}
