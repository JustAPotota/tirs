use core::fmt;
use std::io::Read;

use thiserror::Error;

use crate::{
    util::{u16_from_bytes, u32_from_bytes},
    CalcHandle,
};

#[repr(u8)]
#[derive(Debug)]
pub enum RawPackets {
    RequestBufSize(u32) = 1,
    RespondBufSize(u32) = 2,
    VirtualData(Vec<u8>) = 3,
    FinalVirtData(Vec<u8>) = 4,
    VirtualDataAcknowledge(u16) = 5,
}

impl RawPackets {
    pub fn receive(handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let mut size_buf = [0; 4];
        let mut kind_buf = [0; 1];
        handle.read_exact(&mut size_buf)?;
        handle.read_exact(&mut kind_buf)?;

        let size = u32::from_be_bytes(size_buf);
        let kind = kind_buf[0];

        let mut payload = vec![0; size as usize];
        handle.read_exact(&mut payload)?;

        if let Ok(kind) = RawPacketKind::try_from(kind) {
            println!("TI->PC: Received raw packet {kind:?}",);
        }

        Ok(Self::from_payload(kind, payload)?)
    }

    pub fn receive_exact(kind: RawPacketKind, handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let packet = Self::receive(handle)?;
        if packet.kind() != kind {
            Err(WrongPacketKind {
                expected: kind,
                received: packet.kind(),
            }
            .into())
        } else {
            Ok(packet)
        }
    }

    pub fn from_payload(kind: u8, payload: Vec<u8>) -> Result<Self, UnknownPacketKindError> {
        Ok(match kind {
            1 => Self::RequestBufSize(u32_from_bytes(&payload[0..4])),
            2 => Self::RespondBufSize(u32_from_bytes(&payload[0..4])),
            3 => Self::VirtualData(payload),
            4 => Self::FinalVirtData(payload),
            5 => Self::VirtualDataAcknowledge(u16_from_bytes(&payload[0..2])),
            x => return Err(UnknownPacketKindError(x)),
        })
    }

    pub fn into_payload(self) -> Vec<u8> {
        match self {
            Self::RequestBufSize(size) => size.to_be_bytes().to_vec(),
            Self::RespondBufSize(size) => size.to_be_bytes().to_vec(),
            Self::VirtualData(payload) => payload,
            Self::FinalVirtData(payload) => payload,
            Self::VirtualDataAcknowledge(thing) => thing.to_be_bytes().to_vec(),
        }
    }

    pub fn send(self, handle: &CalcHandle) -> anyhow::Result<()> {
        let kind = self.kind();
        let id = kind as u8;

        let payload = self.into_payload();

        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.push(id);
        bytes.extend_from_slice(&payload);

        println!("PC->TI: Sending raw packet {:?}", kind);

        handle.send(&bytes)?;

        Ok(())
    }

    pub fn kind(&self) -> RawPacketKind {
        match self {
            Self::RequestBufSize { .. } => RawPacketKind::BufSizeReq,
            Self::RespondBufSize { .. } => RawPacketKind::BufSizeAlloc,
            Self::VirtualData { .. } => RawPacketKind::VirtData,
            Self::FinalVirtData { .. } => RawPacketKind::VirtDataLast,
            Self::VirtualDataAcknowledge { .. } => RawPacketKind::VirtDataAck,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum RawPacketKind {
    #[default]
    BufSizeReq = 1,
    BufSizeAlloc = 2,
    VirtData = 3,
    VirtDataLast = 4,
    VirtDataAck = 5,
}

#[derive(Error, Debug)]
#[error("wrong packet kind: expected {expected:?}, received {received:?}")]
pub struct WrongPacketKind {
    pub expected: RawPacketKind,
    pub received: RawPacketKind,
}

#[derive(Error, Debug)]
#[error("wrong packet size: expected {expected:?}, received {received:?}")]
pub struct WrongPacketSize {
    pub expected: u32,
    pub received: u32,
}

#[derive(Error, Debug)]
#[error("invalid payload received")]
pub struct InvalidPayload;

#[derive(Error, Debug)]
pub struct UnknownPacketKindError(u8);
impl fmt::Display for UnknownPacketKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown raw packet kind {}", self.0)
    }
}

impl TryFrom<u8> for RawPacketKind {
    type Error = UnknownPacketKindError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::BufSizeReq),
            2 => Ok(Self::BufSizeAlloc),
            3 => Ok(Self::VirtData),
            4 => Ok(Self::VirtDataLast),
            5 => Ok(Self::VirtDataAck),
            n => Err(UnknownPacketKindError(n)),
        }
    }
}
