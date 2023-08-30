use core::fmt;
use std::io::{Cursor, Read};

use byteorder::{BigEndian, ReadBytesExt};
use strum::{EnumDiscriminants, FromRepr};
use thiserror::Error;

use crate::{
    dusb::{Mode, Parameter, ParameterKind, UnknownParameterKindError},
    util::{u16_from_bytes, u32_from_bytes},
    Calculator,
};

use super::raw::{self, InvalidPayload, RawPacket, RawPacketKind};

#[repr(u16)]
#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(name(VirtualPacketKind))]
#[strum_discriminants(derive(FromRepr))]
pub enum VirtualPacket {
    SetMode(Mode) = 0x0001,
    ParameterRequest(Vec<ParameterKind>) = 0x0007,
    ParameterResponse(Vec<Parameter>) = 0x0008,
    SetModeAcknowledge = 0x0012,
}

impl VirtualPacket {
    pub fn into_payload(self) -> Vec<u8> {
        match self {
            Self::SetMode(mode) => mode.into(),
            Self::ParameterRequest(parameters) => {
                let mut payload = (parameters.len() as u16).to_be_bytes().to_vec();

                for parameter in parameters {
                    payload.extend_from_slice(&(parameter as u16).to_be_bytes());
                }

                payload
            }
            _ => todo!(),
        }
    }

    pub fn into_raw_packets(self, max_size: u32) -> Vec<RawPacket> {
        let kind = VirtualPacketKind::from(&self);
        let contents = self.into_payload();

        let mut bytes = (contents.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(&(kind as u16).to_be_bytes());
        bytes.extend_from_slice(&contents);

        let mut packets = Vec::new();
        let mut chunks = bytes.chunks(max_size as usize).peekable();
        while let Some(chunk) = chunks.next() {
            let is_last = chunks.peek().is_none();
            packets.push(if is_last {
                RawPacket::FinalVirtData(chunk.to_vec())
            } else {
                RawPacket::VirtualData(chunk.to_vec())
            });
        }

        packets
    }

    pub fn send(self, handle: &mut Calculator) -> anyhow::Result<()> {
        println!(
            "PC->TI: Sending virtual packet {:?}",
            VirtualPacketKind::from(&self)
        );
        let packets = self.into_raw_packets(handle.max_raw_packet_size);
        for packet in packets {
            packet.send(handle)?;
            Self::wait_for_acknowledge(handle)?;
        }

        Ok(())
    }

    pub fn wait_for_acknowledge(handle: &mut Calculator) -> anyhow::Result<()> {
        let packet = RawPacket::receive(handle)?;
        match packet {
            RawPacket::RequestBufSize(size) => {
                println!("TI->PC: Buffer Size Request ({size} bytes)");
                RawPacket::RespondBufSize(handle.max_raw_packet_size).send(handle)?;
                Self::wait_for_acknowledge(handle)?;
            }
            RawPacket::VirtualDataAcknowledge(contents) => {
                // It should always have this, no one knows why
                if contents != 0xe000 {
                    return Err(InvalidPayload.into());
                }
            }
            packet => {
                return Err(raw::WrongPacketKind {
                    expected: RawPacketKind::VirtDataAck,
                    received: packet.kind(),
                }
                .into())
            }
        }

        Ok(())
    }

    fn receive_bytes(handle: &mut Calculator) -> anyhow::Result<Vec<u8>> {
        let mut bytes = Vec::new();

        loop {
            match RawPacket::receive(handle)? {
                RawPacket::VirtualData(ref payload) => bytes.extend_from_slice(payload),
                RawPacket::FinalVirtData(ref payload) => {
                    bytes.extend_from_slice(payload);
                    RawPacket::VirtualDataAcknowledge(0xe000).send(handle)?;
                    return Ok(bytes);
                }
                packet => {
                    return Err(raw::WrongPacketKind {
                        expected: RawPacketKind::VirtDataAck,
                        received: packet.kind(),
                    }
                    .into())
                }
            }

            RawPacket::VirtualDataAcknowledge(0xe000).send(handle)?;
        }
    }

    pub fn receive(handle: &mut Calculator) -> anyhow::Result<Self> {
        let bytes = Self::receive_bytes(handle)?;
        let size = u32_from_bytes(&bytes[0..4]);
        let kind = u16_from_bytes(&bytes[4..6]);
        let payload = bytes[6..6 + size as usize].to_vec();

        let kind = VirtualPacketKind::from_repr(kind).ok_or(UnknownPacketKindError(kind))?;
        println!("TI->PC: Received virtual packet {kind:?}");
        Self::from_payload(kind, &payload)
    }

    pub fn from_payload(kind: VirtualPacketKind, payload: &[u8]) -> anyhow::Result<Self> {
        Ok(match kind {
            VirtualPacketKind::SetMode => Self::SetMode(Mode::from(&payload[0..2])),
            VirtualPacketKind::ParameterRequest => {
                let amount = u16_from_bytes(&payload[0..2]) as usize;

                let parameters = payload
                    .chunks_exact(2)
                    .skip(1)
                    .take(amount)
                    .map(|pair| {
                        let id = u16_from_bytes(pair);
                        ParameterKind::from_repr(id).unwrap()
                    })
                    .collect();
                Self::ParameterRequest(parameters)
            }
            VirtualPacketKind::ParameterResponse => {
                let mut parameters = Vec::new();
                let mut payload_cursor = Cursor::new(payload);
                let amount = payload_cursor.read_u16::<BigEndian>()? as usize;
                for _ in 0..amount {
                    let id = payload_cursor.read_u16::<BigEndian>()?;
                    let is_valid = payload_cursor.read_u8()? == 0;
                    if !is_valid {
                        continue;
                    }

                    let parameter_length = {
                        // if the parameter is bigger than u16::MAX, the calc will set the length to 0
                        // stupid dum hack because screenshots on some devices are huge
                        let length = payload_cursor.read_u16::<BigEndian>()? as u32;
                        if length == 0 {
                            153600
                        } else {
                            length
                        }
                    };

                    let mut parameter_data = vec![0; parameter_length as usize];
                    payload_cursor.read_exact(&mut parameter_data)?;

                    let kind = ParameterKind::from_repr(id).ok_or(UnknownParameterKindError(id))?;
                    parameters.push(Parameter::from_payload(kind, &parameter_data));
                }

                Self::ParameterResponse(parameters)
            }
            VirtualPacketKind::SetModeAcknowledge => Self::SetModeAcknowledge,
        })
    }
}

#[derive(Error, Debug)]
pub struct UnknownPacketKindError(pub u16);
impl fmt::Display for UnknownPacketKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown virtual packet kind {}", self.0)
    }
}

#[derive(Error, Debug)]
#[error("wrong packet kind: expected {expected:?}, received {received:?}")]
pub struct WrongPacketKind {
    pub expected: VirtualPacketKind,
    pub received: VirtualPacketKind,
}
