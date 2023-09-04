use core::fmt;
use std::io::{Cursor, Read};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt, BE};
use strum::{EnumDiscriminants, FromRepr};
use thiserror::Error;

use crate::{
    dusb::{
        Mode, Parameter, ParameterKind, UnknownParameterKindError, Variable, VariableAttribute,
        VariableAttributeKind,
    },
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
    DirectoryRequest(Vec<VariableAttributeKind>) = 0x0009,
    VariableHeader(Variable) = 0x000a,
    RequestVariable(String, Vec<VariableAttributeKind>, Vec<VariableAttribute>) = 0x000c,
    VariableContents(Vec<u8>) = 0x000d,
    SetModeAcknowledge = 0x0012,
    Wait(u32) = 0xbb00,
    EndOfTransmission = 0xdd00,
    Error(DeviceError) = 0xee00,
}

#[repr(u16)]
#[derive(Debug, Error, FromRepr)]
pub enum DeviceError {
    #[error("invalid argument")]
    InvalidArgument = 0x04,
    #[error("can't delete app")]
    AppDeleteFail = 0x06,
    #[error("transmission error or invalid code")]
    InvalidCode = 0x08,
    #[error("tried to use basic mode while in boot mode")]
    WrongMode = 0x09,
    #[error("out of memory")]
    OutOfMemory = 0x0c,
    #[error("invalid folder name (?)")]
    InvalidFolderName = 0x0d,
    #[error("invalid name")]
    InvalidName = 0x0e,
    #[error("busy (sent after two keys, remote control?)")]
    Busy = 0x11,
    #[error("variable is locked or archived")]
    VariableUnwritable = 0x12,
    #[error("mode token was too small")]
    ModeTooSmall = 0x1c,
    #[error("mode token was too large")]
    ModeTooLarge = 0x1d,
    #[error("invalid parameter ID or invalid data")]
    InvalidParameter = 0x22,
    #[error("remote control (?)")]
    RemoteControl = 0x29,
    #[error("battery too low to transfer OS")]
    BatteryLow = 0x2b,
    #[error("handheld is busy (not at HOME)")]
    HandheldBusy = 0x34,
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
            Self::DirectoryRequest(attributes) => {
                let mut payload = (attributes.len() as u32).to_be_bytes().to_vec();

                for attribute in attributes {
                    payload.extend_from_slice(&(attribute as u16).to_be_bytes());
                }

                payload.extend_from_slice(&[0, 1, 0, 1, 0, 1, 1]); // idk man

                payload
            }
            Self::RequestVariable(name, requested_attributes, specified_attributes) => {
                let mut payload = (name.len() as u16).to_be_bytes().to_vec();
                payload.extend_from_slice(name.as_bytes());
                payload.extend_from_slice(&[0, 1, 0xff, 0xff, 0xff, 0xff]); // "??????????" - Romain Liévin

                payload.extend_from_slice(&(requested_attributes.len() as u16).to_be_bytes());
                let attr_ids: Vec<u8> = requested_attributes
                    .iter()
                    .flat_map(|attr| (*attr as u16).to_be_bytes())
                    .collect();
                payload.extend_from_slice(&attr_ids);

                payload.extend_from_slice(&(specified_attributes.len() as u16).to_be_bytes());
                for attr in specified_attributes {
                    payload.extend_from_slice(
                        &(VariableAttributeKind::from(attr.clone()) as u16).to_be_bytes(),
                    );
                    let attr_payload = attr.into_payload();
                    payload.extend_from_slice(&(attr_payload.len() as u16).to_be_bytes());
                    payload.extend_from_slice(&attr_payload);
                }

                payload.extend_from_slice(&[0, 0]); // ???

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

    pub fn from_payload(kind: VirtualPacketKind, mut payload: &[u8]) -> anyhow::Result<Self> {
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
                    parameters.push(Parameter::from_payload(kind, &parameter_data)?);
                }

                Self::ParameterResponse(parameters)
            }
            VirtualPacketKind::VariableHeader => {
                let mut payload = Cursor::new(payload);
                let name_length = payload.read_u16::<BE>()?;
                let mut name_bytes = vec![0; name_length as usize];
                payload.read_exact(&mut name_bytes)?;
                payload.read_u8()?; // 0x00
                let attribute_count = payload.read_u16::<BE>()?;
                let name = String::from_utf8_lossy(&name_bytes).into_owned();

                let mut attributes = Vec::new();
                for _ in 0..attribute_count {
                    let id = payload.read_u16::<BE>()?;
                    let is_valid = payload.read_u8()? == 0;

                    if is_valid {
                        let data_length = payload.read_u16::<BE>()?;

                        let mut attribute_data = vec![0; data_length as usize];
                        payload.read_exact(&mut attribute_data)?;

                        attributes.push(VariableAttribute::from_payload(
                            VariableAttributeKind::from_repr(id).unwrap(),
                            &attribute_data,
                        )?);
                    }
                }

                Self::VariableHeader(Variable { name, attributes })
            }
            VirtualPacketKind::VariableContents => Self::VariableContents(payload.to_vec()),
            VirtualPacketKind::SetModeAcknowledge => Self::SetModeAcknowledge,
            VirtualPacketKind::Wait => Self::Wait(payload.read_u32::<BE>()?),
            VirtualPacketKind::EndOfTransmission => Self::EndOfTransmission,
            VirtualPacketKind::Error => {
                Self::Error(DeviceError::from_repr(payload.read_u16::<BE>()?).unwrap())
            }
            _ => todo!(),
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

impl WrongPacketKind {
    pub fn new(expected: VirtualPacketKind, received: VirtualPacket) -> Self {
        Self {
            expected,
            received: received.into(),
        }
    }
}
