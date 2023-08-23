use crate::{
    packet::raw::{self, BufSizeAllocPacket, BufSizeReqPacket, RawPacket, RawPacketKind},
    ticables, CalcHandle,
};

pub const MODE_NORMAL: ModeSet = ModeSet(3, 1, 0, 0x07d0);
pub const DFL_BUF_SIZE: u32 = 1024;
pub const DH_SIZE: u32 = 4 + 2;

#[repr(u16)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VirtualPacketKind {
    Ping = 1,
}

#[derive(Debug, Clone, Copy)]
pub struct ModeSet(u16, u16, u16, u32);
const MODE_SET_SIZE: u32 = std::mem::size_of::<ModeSet>() as u32;

#[derive(Debug, Clone)]
pub struct VirtualPacket {
    pub size: u32,
    pub kind: VirtualPacketKind,
    pub data: Vec<u8>,
}

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

fn u32_from_slice(slice: &[u8]) -> u32 {
    let array: [u8; 4] = slice.try_into().unwrap();
    u32::from_be_bytes(array)
}

pub fn cmd_send_mode_set(handle: &mut CalcHandle, mode: ModeSet) -> anyhow::Result<()> {
    send_buf_size_request(handle, DFL_BUF_SIZE)?;
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

fn send_buf_size_request(handle: &CalcHandle, size: u32) -> anyhow::Result<()> {
    let packet = BufSizeReqPacket::new(size);
    packet.send(handle)?;

    Ok(())
}

pub fn read_buf_size_alloc(handle: &mut CalcHandle) -> anyhow::Result<u32> {
    let packet = BufSizeAllocPacket::receive(handle)?;
    println!("Device max buffer size: {}", packet.size);
    handle.max_raw_packet_size = packet.size;

    Ok(packet.size)
}

pub fn write_buf_size_alloc(handle: &mut CalcHandle, size: u32) -> anyhow::Result<()> {
    let packet = raw::RawPacket::new(
        raw::RawPacketKind::BufSizeAlloc,
        size.to_be_bytes().to_vec(),
    );
    packet.send(handle)?;

    handle.max_raw_packet_size = size;

    Ok(())
}

// pub fn virtual_packet_new_ex<T: UsbContext>(
//     handle: &DeviceHandle<T>,
//     size: u32,
//     kind: VirtualPacketKind,
//     data: &[u8],
// ) -> VirtualPacket {

// }

// pub fn read(handle: &mut CalcHandle) -> anyhow::Result<RawPacket> {
//     // Read header
//     let mut buf = [0; 5];
//     crate::ticables::cable_read(handle, &mut buf, 5)?;

//     let size = u32::from_be_bytes(*slice_to_array(&buf[..4]));
//     let mut packet = RawPacket::new(buf[4].try_into().unwrap(), vec![0; size as usize]);

//     // Read payload
//     crate::ticables::cable_read(handle, &mut packet.data, size as usize)?;

//     Ok(raw)
// }

fn send(handle: &CalcHandle, packet: &RawPacket) -> anyhow::Result<()> {
    let size = packet.size() as u32;
    println!("Sending packet of size {size}...");
    let mut buf = size.to_be_bytes().to_vec();
    buf.push(packet.kind as u8);
    buf.append(&mut packet.payload.clone());
    ticables::write(handle, &buf, 0)?;
    // handle
    //     .device
    //     .write_interrupt(2, &buf, Duration::from_secs(10))?;
    Ok(())
}

pub fn send_data(handle: &mut CalcHandle, packet: VirtualPacket) -> anyhow::Result<()> {
    if packet.size <= handle.max_raw_packet_size - DH_SIZE {
        // Single packet
        let mut raw_packet = RawPacket::new(
            RawPacketKind::VirtDataLast,
            vec![0; packet.data.len() + DH_SIZE as usize],
        );

        raw_packet.payload[0..4].copy_from_slice(&packet.size.to_be_bytes());
        raw_packet.payload[4..6].copy_from_slice(&(packet.kind as u16).to_be_bytes());
        raw_packet.payload[6..packet.data.len() + DH_SIZE as usize]
            .copy_from_slice(&packet.data[..]);
        for (i, byte) in packet.data.iter().enumerate() {
            raw_packet.payload[i + DH_SIZE as usize] = *byte;
        }

        send(handle, &raw_packet)?;
        workaround_send(handle, raw_packet, packet)?;
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
        offset += remaining_data_len as usize;

        last_packet.send(handle)?;
        // maybe workaround_send()
        read_acknowledge(handle)?;
    }
    Ok(())
}

fn workaround_send(
    handle: &CalcHandle,
    raw_packet: RawPacket,
    virtual_packet: VirtualPacket,
) -> anyhow::Result<()> {
    let buf = vec![0; 64];

    if true
    /*handle.model == TI84PCE_USB*/
    {
        if raw_packet.kind == RawPacketKind::VirtDataLast
            && ((raw_packet.payload.len() + 5) % 64) == 0
        {
            println!(
                "Triggering an extra bulk write\n\tvirtual size: {}\t raw size: {}",
                virtual_packet.size,
                raw_packet.payload.len()
            );
            ticables::write(handle, &buf, 0)?;
        }
    }

    Ok(())
}

pub fn read_acknowledge(handle: &mut CalcHandle) -> anyhow::Result<()> {
    let mut packet = RawPacket::receive(handle)?;
    let packet_size = packet.payload.len();

    if packet_size != 2 && packet_size != 4 {
        println!("Invalid packet");
    }

    if packet.kind == RawPacketKind::BufSizeReq {
        if packet_size != 4 {
            println!("Invalid packet");
        }
        let size = u32_from_slice(&packet.payload[0..4]);
        println!("  TI->PC:  Buffer Size Request ({size} bytes)");

        write_buf_size_alloc(handle, size)?;

        packet = RawPacket::receive(handle)?;
    }

    if packet.kind != RawPacketKind::VirtDataAck {
        println!("Invalid packet");
    }

    if packet.payload[0] != 0xe0 && packet.payload[1] != 0x00 {
        println!("Invalid packet");
    }

    Ok(())
}
