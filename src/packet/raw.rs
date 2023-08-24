use core::fmt;
use std::io::Read;

use thiserror::Error;

use crate::{
    util::{u16_from_bytes, u32_from_bytes},
    CalcHandle,
};

use super::vtl::VirtualPacketKind;

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

pub struct RawPacketHeader {
    pub size: u32,
    pub kind: RawPacketKind,
}

impl RawPacketHeader {
    pub fn receive(handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let mut size_buf = [0; 4];
        let mut kind_buf = [0; 1];
        handle.read_exact(&mut size_buf)?;
        handle.read_exact(&mut kind_buf)?;

        Ok(Self {
            size: u32::from_be_bytes(size_buf),
            kind: kind_buf[0].try_into()?,
        })
    }
}

pub trait RawPacketTrait: Sized {
    const KIND: RawPacketKind;
    const ID: u8;

    fn payload(&self) -> Vec<u8>;
    fn is_valid(payload: &[u8]) -> bool;
    fn from_payload(payload: &[u8]) -> anyhow::Result<Self>;

    fn send(&self, handle: &CalcHandle) -> anyhow::Result<()> {
        let payload = self.payload();
        let mut bytes = (self.payload().len() as u32).to_be_bytes().to_vec();
        bytes.push(Self::ID);
        bytes.extend_from_slice(&payload);
        handle.send(&bytes)?;

        println!("Sent {:?} payload", Self::KIND);

        Ok(())
    }

    fn receive(handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let header = RawPacketHeader::receive(handle)?;
        // Not checking the type yet to make sure we consume all sent bytes
        let mut payload = vec![0; header.size as usize];
        handle.read_exact(&mut payload)?;

        if header.kind != Self::KIND {
            return Err(WrongPacketKind {
                expected: Self::KIND,
                received: header.kind,
            }
            .into());
        }

        if !Self::is_valid(&payload) {
            return Err(InvalidPayload.into());
        }

        println!("Received {:?} payload", Self::KIND);

        Self::from_payload(&payload)
    }
}

pub struct RawPacket {
    pub kind: RawPacketKind,
    pub payload: Vec<u8>,
}

impl RawPacket {
    pub fn new(kind: RawPacketKind, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    pub fn send(&self, handle: &CalcHandle) -> anyhow::Result<()> {
        let mut bytes = (self.payload.len() as u32).to_be_bytes().to_vec();
        bytes.push(self.kind as u8);
        bytes.append(&mut self.payload.clone());
        handle.send(&bytes)?;
        Ok(())
    }

    pub fn receive(handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let mut buf = [0; 4];
        handle.read_exact(&mut buf)?;
        let size = u32::from_be_bytes(buf);
        let mut buf = [0; 1];
        handle.read_exact(&mut buf)?;
        let kind = RawPacketKind::try_from(buf[0])?;
        let mut payload = vec![0; size as usize];
        handle.read_exact(&mut payload)?;

        Ok(Self { kind, payload })
    }
}

#[derive(Debug)]
pub struct BufSizeReqPacket {
    pub size: u32,
}

impl RawPacketTrait for BufSizeReqPacket {
    const KIND: RawPacketKind = RawPacketKind::BufSizeReq;
    const ID: u8 = 1;

    fn payload(&self) -> Vec<u8> {
        self.size.to_be_bytes().to_vec()
    }

    fn from_payload(payload: &[u8]) -> anyhow::Result<Self> {
        Ok(Self {
            size: u32_from_bytes(&payload[0..4]),
        })
    }

    fn is_valid(payload: &[u8]) -> bool {
        payload.len() == 4
    }
}

impl BufSizeReqPacket {
    pub fn new(size: u32) -> Self {
        Self { size }
    }
}

#[derive(Debug)]
pub struct FinalVirtDataPacket {
    pub packet_kind: VirtualPacketKind,
    pub virtual_payload: Vec<u8>,
}

impl RawPacketTrait for FinalVirtDataPacket {
    const KIND: RawPacketKind = RawPacketKind::VirtDataLast;
    const ID: u8 = 4;

    fn payload(&self) -> Vec<u8> {
        let mut payload = (self.virtual_payload.len() as u32).to_be_bytes().to_vec();
        payload.extend_from_slice(&(self.packet_kind as u16).to_be_bytes());
        payload.extend_from_slice(&self.virtual_payload);

        payload
    }

    fn from_payload(payload: &[u8]) -> anyhow::Result<Self> {
        let size = u32_from_bytes(&payload[0..4]);
        let kind = VirtualPacketKind::try_from(u16_from_bytes(&payload[4..6]))?;
        let payload = payload[6..6 + size as usize].to_vec();

        Ok(Self {
            packet_kind: kind,
            virtual_payload: payload,
        })
    }

    fn is_valid(payload: &[u8]) -> bool {
        payload.len() >= 6
    }
}

impl FinalVirtDataPacket {
    pub fn new(packet_kind: VirtualPacketKind, virtual_payload: Vec<u8>) -> Self {
        Self {
            virtual_payload,
            packet_kind,
        }
    }
}

#[derive(Debug)]
pub struct BufSizeAllocPacket {
    pub size: u32,
}

impl RawPacketTrait for BufSizeAllocPacket {
    const KIND: RawPacketKind = RawPacketKind::BufSizeAlloc;
    const ID: u8 = 2;

    fn payload(&self) -> Vec<u8> {
        self.size.to_be_bytes().to_vec()
    }

    fn from_payload(payload: &[u8]) -> anyhow::Result<Self> {
        let mut size = u32_from_bytes(&payload[0..4]);

        if size > 1018 {
            println!(
                "The 83PCE/84+CE allocate more than they support. Clamping buffer size to 1018"
            );
            size = 1018;
        }

        Ok(Self { size })
    }

    fn is_valid(payload: &[u8]) -> bool {
        payload.len() == 4
    }
}

impl BufSizeAllocPacket {
    pub fn new(size: u32) -> Self {
        Self { size }
    }
}
