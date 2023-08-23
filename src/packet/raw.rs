use core::fmt;
use std::io::Read;

use thiserror::Error;

use crate::CalcHandle;

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

#[derive(Debug)]
pub struct UnknownPacketKindError(u8);
impl fmt::Display for UnknownPacketKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown raw packet kind {}", self.0)
    }
}
impl std::error::Error for UnknownPacketKindError {}

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

pub trait RawPacketTrait {
    const KIND: RawPacketKind;
}

pub trait SendPacket: RawPacketTrait {
    fn payload(&self) -> Vec<u8>;
}

impl<P> From<P> for RawPacket
where
    P: SendPacket,
{
    fn from(packet: P) -> Self {
        Self::new(P::KIND, packet.payload())
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

    pub fn size(&self) -> usize {
        self.payload.len()
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
}

impl SendPacket for BufSizeReqPacket {
    fn payload(&self) -> Vec<u8> {
        self.size.to_be_bytes().to_vec()
    }
}

impl BufSizeReqPacket {
    pub fn new(size: u32) -> Self {
        Self { size }
    }

    pub fn send(&self, handle: &CalcHandle) -> anyhow::Result<()> {
        let packet = RawPacket::new(RawPacketKind::BufSizeReq, self.size.to_be_bytes().to_vec());
        packet.send(handle)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct BufSizeAllocPacket {
    pub size: u32,
}

impl BufSizeAllocPacket {
    pub fn receive(handle: &mut CalcHandle) -> anyhow::Result<Self> {
        let header = RawPacketHeader::receive(handle)?;
        if header.kind != RawPacketKind::BufSizeAlloc {
            return Err(WrongPacketKind {
                expected: RawPacketKind::BufSizeAlloc,
                received: header.kind,
            }
            .into());
        }
        if header.size != 4 {
            return Err(WrongPacketSize {
                expected: 4,
                received: header.size,
            }
            .into());
        }
        let mut buf = [0; 4];
        handle.read_exact(&mut buf)?;
        let mut size = u32::from_be_bytes(buf);
        if size > 1018 {
            println!(
                "The 83PCE/84+CE allocate more than they support. Clamping buffer size to 1018"
            );
            size = 1018;
        }
        Ok(Self { size })
    }
}
