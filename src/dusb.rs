use crate::{
    packet::{
        raw::{RawPacketKind, RawPackets, WrongPacketKind},
        vtl::{VirtualPacket, VirtualPacketKind},
    },
    CalcHandle,
};

pub const MODE_NORMAL: ModeSet = ModeSet(3, 1, 0, 0x07d0);
pub const DFL_BUF_SIZE: u32 = 1024;

#[derive(Debug, Clone, Copy)]
pub struct ModeSet(u16, u16, u16, u32);
const MODE_SET_SIZE: u32 = std::mem::size_of::<ModeSet>() as u32;

impl From<ModeSet> for [u8; 10] {
    fn from(value: ModeSet) -> Self {
        let mut bytes = [0; 10];
        bytes[0..2].copy_from_slice(&value.0.to_be_bytes());
        bytes[2..4].copy_from_slice(&value.1.to_be_bytes());
        bytes[4..6].copy_from_slice(&value.2.to_be_bytes());
        bytes[6..10].copy_from_slice(&value.3.to_be_bytes());
        bytes
    }
}

impl From<ModeSet> for Vec<u8> {
    fn from(value: ModeSet) -> Self {
        let mode: [u8; 10] = value.into();
        mode.to_vec()
    }
}

pub fn cmd_send_mode_set(handle: &mut CalcHandle, mode: ModeSet) -> anyhow::Result<()> {
    RawPackets::RequestBufSize(DFL_BUF_SIZE).send(handle)?;

    read_buf_size_alloc(handle)?;
    let packet = VirtualPacket {
        size: MODE_SET_SIZE,
        kind: VirtualPacketKind::SetMode,
        payload: mode.into(),
    };
    packet.send(handle)?;

    Ok(())
}

pub fn read_buf_size_alloc(handle: &mut CalcHandle) -> anyhow::Result<u32> {
    let packet = RawPackets::receive_exact(RawPacketKind::BufSizeAlloc, handle)?;
    match packet {
        RawPackets::RespondBufSize(mut size) => {
            println!("TI->PC: Responded with buffer size {size}");
            if size > 1018 {
                println!(
                    "[The 83PCE/84+CE allocate more than they support. Clamping buffer size to 1018]"
                );
                size = 1018;
            };
            handle.max_raw_packet_size = size;
            Ok(size)
        }
        packet => Err(WrongPacketKind {
            expected: RawPacketKind::BufSizeAlloc,
            received: packet.kind(),
        }
        .into()),
    }
}
