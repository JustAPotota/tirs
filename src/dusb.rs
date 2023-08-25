use crate::{
    packet::{
        raw::{RawPacket, RawPacketKind, WrongPacketKind},
        vtl::{VirtualPacket, VirtualPacketKind},
    },
    CalcHandle,
};

#[repr(u8)]
pub enum Mode {
    Startup = 1,
    Basic = 2,
    Normal = 3,
}

impl From<Mode> for [u8; 10] {
    fn from(value: Mode) -> Self {
        let id = value as u8;
        [0, id, 0, 1, 0, 0, 0, 0, 0x7d, 0xd0]
    }
}

pub const DFL_BUF_SIZE: u32 = 1024;

pub fn cmd_send_mode_set(handle: &mut CalcHandle, mode: Mode) -> anyhow::Result<()> {
    RawPacket::RequestBufSize(DFL_BUF_SIZE).send(handle)?;

    read_buf_size_alloc(handle)?;
    let mode: [u8; 10] = mode.into();
    let packet = VirtualPacket {
        size: 10,
        kind: VirtualPacketKind::SetMode,
        payload: mode.to_vec(),
    };
    packet.send(handle)?;

    println!("{:?}", VirtualPacket::receive(handle)?);

    Ok(())
}

pub fn read_buf_size_alloc(handle: &mut CalcHandle) -> anyhow::Result<u32> {
    let packet = RawPacket::receive_exact(RawPacketKind::BufSizeAlloc, handle)?;
    match packet {
        RawPacket::RespondBufSize(mut size) => {
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
