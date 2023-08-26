use core::fmt;

use strum::{EnumDiscriminants, FromRepr};
use thiserror::Error;

use crate::{
    packet::{
        raw::{RawPacket, RawPacketKind, WrongPacketKind},
        vtl::{self, VirtualPacket, VirtualPacketKind},
    },
    util::i32_from_bytes,
    CalcHandle,
};

#[repr(u8)]
#[derive(Debug, FromRepr)]
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

impl From<Mode> for Vec<u8> {
    fn from(value: Mode) -> Self {
        <[u8; 10]>::from(value).to_vec()
    }
}

impl From<&[u8]> for Mode {
    fn from(value: &[u8]) -> Self {
        Self::from_repr(value[1]).unwrap()
    }
}

pub const DFL_BUF_SIZE: u32 = 1024;

#[repr(u16)]
pub enum VariableAttribute {
    Size = 1,
    Kind = 2,
}

pub struct Variable {
    pub name: String,
    pub attributes: Vec<VariableAttribute>,
}

#[repr(u16)]
#[derive(Debug, FromRepr, EnumDiscriminants)]
#[strum_discriminants(name(ParameterKind))]
#[strum_discriminants(derive(FromRepr))]
pub enum Parameter {
    Name(String) = 0x02,
    Clock(i32) = 0x25,
}

impl Parameter {
    pub fn from_payload(kind: ParameterKind, payload: &[u8]) -> Self {
        match kind {
            ParameterKind::Name => Self::Name(String::from_utf8_lossy(payload).into_owned()),
            ParameterKind::Clock => Self::Clock(i32_from_bytes(&payload[0..4])),
        }
    }
}

#[derive(Error, Debug)]
pub struct UnknownParameterKindError(pub u16);
impl fmt::Display for UnknownParameterKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown parameter kind {}", self.0)
    }
}

pub fn negotiate() {}

pub fn set_mode(handle: &mut CalcHandle, mode: Mode) -> anyhow::Result<()> {
    RawPacket::RequestBufSize(DFL_BUF_SIZE).send(handle)?;

    read_buf_size_alloc(handle)?;
    VirtualPacket::SetMode(mode).send(handle)?;
    // let mode: [u8; 10] = mode.into();
    // let packet = VirtualPacket {
    //     size: 10,
    //     kind: VirtualPacketKind::SetMode,
    //     payload: mode.to_vec(),
    // };
    // packet.send(handle)?;
    let packet = VirtualPacket::receive(handle)?;
    println!("{packet:?}");
    //println!("{:?}", VirtualPacket::receive(handle)?);

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

pub fn request_directory_listing(handle: &mut CalcHandle) -> anyhow::Result<()> {
    todo!()
}

pub fn request_parameters(
    handle: &mut CalcHandle,
    parameters: &[ParameterKind],
) -> anyhow::Result<Vec<Parameter>> {
    RawPacket::RequestBufSize(DFL_BUF_SIZE).send(handle)?;
    read_buf_size_alloc(handle)?;

    println!("PC->TI: Requesting parameters {parameters:?}");

    VirtualPacket::ParameterRequest(parameters.to_vec()).send(handle)?;

    Ok(match VirtualPacket::receive(handle)? {
        VirtualPacket::ParameterResponse(parameters) => parameters,
        packet => {
            return Err(vtl::WrongPacketKind {
                expected: VirtualPacketKind::ParameterResponse,
                received: packet.into(),
            }
            .into())
        }
    })
}
