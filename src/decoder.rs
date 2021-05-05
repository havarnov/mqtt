use bytes::{Buf, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use super::protocol::MqttPacket;
use crate::parser::parse_mqtt;
use crate::protocol::to_bytes;
use std::fmt::{Display, Formatter};
use std::io::Error;

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
        write!(f, "foobar")
    }
}

impl std::error::Error for MqttPacketDecoderError {}

impl Encoder<MqttPacket> for MqttPacketDecoder {
    type Error = MqttPacketDecoderError;

    fn encode(&mut self, item: MqttPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let bytes = to_bytes(&item);
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

        println!("{:?}", src);

        match parse_mqtt(&src) {
            Ok((rest, packet)) => {
                // TODO: fix mutable warning
                src.advance(src.len() - rest.len());
                Ok(Some(packet))
            }
            Err(nom::Err::Incomplete(_)) => Ok(None),
            Err(nom::Err::Failure(_)) => Err(MqttPacketDecoderError {}),
            _ => Err(MqttPacketDecoderError {}),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
