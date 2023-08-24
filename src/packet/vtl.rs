use core::fmt;

use thiserror::Error;

use crate::CalcHandle;

use super::raw::{InvalidPayload, RawPacketKind, RawPackets, WrongPacketKind};

#[repr(u16)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VirtualPacketKind {
    SetMode = 1,
}

#[derive(Error, Debug)]
pub struct UnknownPacketKindError(u16);
impl fmt::Display for UnknownPacketKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown raw packet kind {}", self.0)
    }
}

impl TryFrom<u16> for VirtualPacketKind {
    type Error = UnknownPacketKindError;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SetMode),
            n => Err(UnknownPacketKindError(n)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualPacket {
    pub size: u32,
    pub kind: VirtualPacketKind,
    pub payload: Vec<u8>,
}

impl VirtualPacket {
    pub fn split(self, max_size: u32) -> Vec<RawPackets> {
        let mut bytes = (self.payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(&(self.kind as u16).to_be_bytes());
        bytes.extend_from_slice(&self.payload);

        let mut packets = Vec::new();
        let mut chunks = bytes.chunks(max_size as usize).peekable();
        while let Some(chunk) = chunks.next() {
            let is_last = chunks.peek().is_none();
            packets.push(if is_last {
                RawPackets::FinalVirtData(chunk.to_vec())
            } else {
                RawPackets::VirtualData(chunk.to_vec())
            });
        }

        packets
    }

    fn receive_acknowledge(handle: &mut CalcHandle) -> anyhow::Result<()> {
        let packet = RawPackets::receive(handle)?;
        match packet {
            RawPackets::RequestBufSize(size) => {
                println!("TI->PC: Buffer Size Request ({size} bytes)");
                RawPackets::RespondBufSize(handle.max_raw_packet_size).send(handle)?;
                Self::receive_acknowledge(handle)?;
            }
            RawPackets::VirtualDataAcknowledge(contents) => {
                // It should always have this, no one knows why
                if contents != 0xe000 {
                    return Err(InvalidPayload.into());
                }
            }
            packet => {
                return Err(WrongPacketKind {
                    expected: RawPacketKind::VirtDataAck,
                    received: packet.kind(),
                }
                .into())
            }
        }

        Ok(())
    }

    pub fn send(self, handle: &mut CalcHandle) -> anyhow::Result<()> {
        let packets = self.split(handle.max_raw_packet_size);
        for packet in packets {
            packet.send(handle)?;
            Self::receive_acknowledge(handle)?;
        }

        Ok(())
    }
}
