use crate::{
    packet::{
        raw::{
            BufSizeAllocPacket, BufSizeReqPacket, FinalVirtDataPacket, InvalidPayload, Packet,
            RawPacket, RawPacketKind, RawPacketTrait, WrongPacketKind,
        },
        vtl::{VirtualPacket, VirtualPacketKind},
    },
    CalcHandle,
};

pub const MODE_NORMAL: ModeSet = ModeSet(3, 1, 0, 0x07d0);
pub const DFL_BUF_SIZE: u32 = 1024;
pub const DH_SIZE: u32 = 4 + 2;

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
    let buf_size_request = BufSizeReqPacket::new(DFL_BUF_SIZE);
    buf_size_request.send(handle)?;

    read_buf_size_alloc(handle)?;
    let mut packet = VirtualPacket {
        size: MODE_SET_SIZE,
        kind: VirtualPacketKind::Ping,
        data: mode.into(),
    };
    packet.data.resize((MODE_SET_SIZE + DH_SIZE) as usize, 0);

    send_data(handle, packet)?;

    Ok(())
}

pub fn read_buf_size_alloc(handle: &mut CalcHandle) -> anyhow::Result<u32> {
    let packet = BufSizeAllocPacket::receive(handle)?;
    println!("Device max buffer size: {}", packet.size);
    handle.max_raw_packet_size = packet.size;

    Ok(packet.size)
}

pub fn write_buf_size_alloc(handle: &mut CalcHandle, size: u32) -> anyhow::Result<()> {
    let packet = BufSizeAllocPacket::new(size);
    packet.send(handle)?;

    handle.max_raw_packet_size = size;

    Ok(())
}

pub fn send_data(handle: &mut CalcHandle, packet: VirtualPacket) -> anyhow::Result<()> {
    if packet.size <= handle.max_raw_packet_size - DH_SIZE {
        // Single packet
        let raw_packet = FinalVirtDataPacket::new(packet.kind, packet.data.clone());
        raw_packet.send(handle)?;
        //workaround_send(handle, raw_packet, packet)?;
        read_acknowledge(handle)?;
    } else {
        // More than one packet, starting with header
        let mut first_packet = RawPacket::new(
            RawPacketKind::VirtData,
            vec![0; handle.max_raw_packet_size as usize],
        );

        first_packet.payload[0..4].copy_from_slice(&packet.size.to_be_bytes());
        first_packet.payload[4..6].copy_from_slice(&(packet.kind as u16).to_be_bytes());
        let mut offset = (handle.max_raw_packet_size - DH_SIZE) as usize;
        first_packet.payload[6..offset].copy_from_slice(&packet.data[..offset]);

        first_packet.send(handle)?;
        read_acknowledge(handle)?;

        let packets_to_send = (packet.size - offset as u32) / handle.max_raw_packet_size;
        let remaining_data_len = (packet.size - offset as u32) % handle.max_raw_packet_size;

        // Then as many max size packets as needed
        for _ in 1..=packets_to_send {
            let mut next_packet = RawPacket::new(
                RawPacketKind::VirtData,
                vec![0; handle.max_raw_packet_size as usize],
            );

            let packet_size = handle.max_raw_packet_size as usize;
            next_packet.payload[..packet_size]
                .copy_from_slice(&packet.data[offset..offset + packet_size]);
            offset += packet_size;

            next_packet.send(handle)?;
            read_acknowledge(handle)?;
        }

        // Then the last chunk
        let last_packet = RawPacket::new(
            RawPacketKind::VirtDataLast,
            packet.data[offset..offset + remaining_data_len as usize].to_owned(),
        );
        //offset += remaining_data_len as usize;

        last_packet.send(handle)?;
        // maybe workaround_send()
        read_acknowledge(handle)?;
    }
    Ok(())
}

// fn workaround_send(
//     handle: &CalcHandle,
//     raw_packet: RawPacket,
//     virtual_packet: VirtualPacket,
// ) -> anyhow::Result<()> {
//     let buf = vec![0; 64];

//     if true
//     /*handle.model == TI84PCE_USB*/
//     {
//         if raw_packet.kind == RawPacketKind::VirtDataLast
//             && ((raw_packet.payload.len() + 5) % 64) == 0
//         {
//             println!(
//                 "Triggering an extra bulk write\n\tvirtual size: {}\t raw size: {}",
//                 virtual_packet.size,
//                 raw_packet.payload.len()
//             );
//             ticables::write(handle, &buf, 0)?;
//         }
//     }

//     Ok(())
// }

pub fn read_acknowledge(handle: &mut CalcHandle) -> anyhow::Result<()> {
    let packet = Packet::receive(handle)?;
    match packet {
        Packet::BufSizeReq { size } => {
            println!("TI->PC: Buffer Size Request ({size} bytes)");
            write_buf_size_alloc(handle, size)?;
            read_acknowledge(handle)
        }
        Packet::VirtualDataAcknowledge(contents) => {
            if contents != 0xe000 {
                Err(InvalidPayload.into())
            } else {
                Ok(())
            }
        }
        packet => Err(WrongPacketKind {
            expected: RawPacketKind::VirtDataAck,
            received: packet.kind(),
        }
        .into()),
    }
}
