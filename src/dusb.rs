use std::time::Duration;

use rusb::{DeviceHandle, UsbContext};

use crate::{ticables, CalcHandle};

pub const MODE_NORMAL: ModeSet = ModeSet(3, 1, 0, 0, 0x07d0);
pub const DFL_BUF_SIZE: u32 = 1024;
pub const DH_SIZE: u32 = 4 + 2;

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

#[repr(u16)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VirtualPacketKind {
    Ping = 1,
}

impl TryFrom<u8> for RawPacketKind {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::BufSizeReq),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModeSet(u16, u16, u16, u16, u16);
const MODE_SET_SIZE: u32 = std::mem::size_of::<ModeSet>() as u32;

#[derive(Debug, Default, Clone)]
pub struct RawPacket {
    pub size: u32,
    pub kind: RawPacketKind,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct VirtualPacket {
    pub size: u32,
    pub kind: VirtualPacketKind,
    pub data: Vec<u8>,
}

impl From<ModeSet> for [u8; 10] {
    fn from(value: ModeSet) -> Self {
        [
            value.0.to_be_bytes(),
            value.1.to_be_bytes(),
            value.2.to_be_bytes(),
            value.3.to_be_bytes(),
            value.4.to_be_bytes(),
        ]
        .concat()
        .try_into()
        .unwrap()
    }
}

impl From<ModeSet> for Vec<u8> {
    fn from(value: ModeSet) -> Self {
        [
            value.0.to_be_bytes(),
            value.1.to_be_bytes(),
            value.2.to_be_bytes(),
            value.3.to_be_bytes(),
            value.4.to_be_bytes(),
        ]
        .concat()
    }
}

fn u32_from_slice(slice: &[u8]) -> u32 {
    let array: [u8; 4] = slice.try_into().unwrap();
    u32::from_be_bytes(array)
}

fn slice_to_array(slice: &[u8]) -> &[u8; 4] {
    slice.try_into().unwrap()
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
    let raw_packet = RawPacket {
        size: 4,
        kind: RawPacketKind::BufSizeReq,
        data: size.to_be_bytes().to_vec(),
    };
    send(handle, &raw_packet)?;
    Ok(())
}

pub fn read_buf_size_alloc(handle: &mut CalcHandle) -> anyhow::Result<u32> {
    let raw = read(handle)?;
    if raw.size != 4 || raw.kind != RawPacketKind::BufSizeAlloc {
        eprintln!("Invalid packet!");
    }
    let mut size = u32_from_slice(&raw.data[..4]);
    if size > 1018 {
        println!("The 83PCE/84+CE allocate more than they support. Clamping buffer size to 1018");
        size = 1018;
    }
    handle.max_raw_packet_size = size;

    Ok(size)
}

pub fn write_buf_size_alloc(handle: &mut CalcHandle, size: u32) -> anyhow::Result<()> {
    let raw_packet = RawPacket {
        size: 4,
        kind: RawPacketKind::BufSizeAlloc,
        data: size.to_be_bytes().to_vec(),
    };

    send(handle, &raw_packet)?;

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

pub fn read(handle: &CalcHandle) -> anyhow::Result<RawPacket> {
    // Read header
    let mut buf = [0; 5];
    crate::ticables::cable_read(handle, &mut buf, 5)?;

    let size = u32::from_be_bytes(*slice_to_array(&buf[..4]));
    let mut raw = RawPacket {
        size,
        kind: buf[4].try_into().unwrap(),
        data: vec![0; size as usize],
    };

    // Read payload
    crate::ticables::cable_read(handle, &mut raw.data, size as usize)?;

    Ok(raw)
}

fn send(handle: &CalcHandle, packet: &RawPacket) -> anyhow::Result<()> {
    let size = packet.size.min(packet.data.len() as u32);
    println!("Sending packet of size {size}...");
    let mut buf = size.to_be_bytes().to_vec();
    buf.push(packet.kind as u8);
    buf.append(&mut packet.data.clone());
    ticables::write(handle, &buf, 0)?;
    // handle
    //     .device
    //     .write_interrupt(2, &buf, Duration::from_secs(10))?;
    Ok(())
}

pub fn send_data(handle: &mut CalcHandle, packet: VirtualPacket) -> anyhow::Result<()> {
    let mut raw_packet = RawPacket::default();
    if packet.size <= handle.max_raw_packet_size - DH_SIZE {
        // Single packet
        raw_packet.size = packet.size + DH_SIZE;
        raw_packet.kind = RawPacketKind::VirtDataLast;

        raw_packet.data[0..4].copy_from_slice(&packet.size.to_be_bytes());
        raw_packet.data[4..6].copy_from_slice(&(packet.kind as u16).to_be_bytes());
        raw_packet.data[6..packet.data.len() + DH_SIZE as usize].copy_from_slice(&packet.data[..]);
        for (i, byte) in packet.data.iter().enumerate() {
            raw_packet.data[i + DH_SIZE as usize] = *byte;
        }

        send(handle, &raw_packet)?;
        workaround_send(handle, raw_packet, packet)?;
        read_acknowledge(handle)?;
    } else {
        // More than one packet, starting with header
        let mut first_packet = RawPacket {
            size: handle.max_raw_packet_size,
            kind: RawPacketKind::VirtData,
            data: vec![0; handle.max_raw_packet_size as usize],
        };

        first_packet.data[0..4].copy_from_slice(&packet.size.to_be_bytes());
        first_packet.data[4..6].copy_from_slice(&(packet.kind as u16).to_be_bytes());
        let mut offset = (handle.max_raw_packet_size - DH_SIZE) as usize;
        first_packet.data[6..offset].copy_from_slice(&packet.data[..offset]);

        send(handle, &first_packet)?;
        read_acknowledge(handle)?;

        let packets_to_send = (packet.size - offset as u32) / handle.max_raw_packet_size;
        let remaining_data_len = (packet.size - offset as u32) % handle.max_raw_packet_size;

        // Then as many max size packets as needed
        for _ in 1..=packets_to_send {
            let mut next_packet = RawPacket {
                size: handle.max_raw_packet_size,
                kind: RawPacketKind::VirtData,
                data: vec![0; handle.max_raw_packet_size as usize],
            };

            let packet_size = handle.max_raw_packet_size as usize;
            next_packet.data[..packet_size]
                .copy_from_slice(&packet.data[offset..offset + packet_size]);
            offset += packet_size;

            send(handle, &next_packet)?;
            read_acknowledge(handle)?;
        }

        // Then the last chunk
        let last_packet = RawPacket {
            size: remaining_data_len,
            kind: RawPacketKind::VirtDataLast,
            data: packet.data[offset..offset + remaining_data_len as usize].to_owned(),
        };
        offset += remaining_data_len as usize;

        send(handle, &last_packet)?;
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
        if raw_packet.kind == RawPacketKind::VirtDataLast && ((raw_packet.size + 5) % 64) == 0 {
            println!(
                "Triggering an extra bulk write\n\tvirtual size: {}\t raw size: {}",
                virtual_packet.size, raw_packet.size
            );
            ticables::write(handle, &buf, 0)?;
        }
    }

    Ok(())
}

pub fn read_acknowledge(handle: &mut CalcHandle) -> anyhow::Result<()> {
    let mut raw_packet = read(handle)?;
    if raw_packet.size != 2 && raw_packet.size != 4 {
        println!("Invalid packet");
    }

    if raw_packet.kind == RawPacketKind::BufSizeReq {
        if raw_packet.size != 4 {
            println!("Invalid packet");
        }
        let size = u32_from_slice(&raw_packet.data[0..4]);
        println!("  TI->PC:  Buffer Size Request ({size} bytes)");

        write_buf_size_alloc(handle, size)?;

        raw_packet = read(handle)?;
    }

    if raw_packet.kind != RawPacketKind::VirtDataAck {
        println!("Invalid packet");
    }

    if raw_packet.data[0] != 0xe0 && raw_packet.data[1] != 0x00 {
        println!("Invalid packet");
    }

    Ok(())
}
