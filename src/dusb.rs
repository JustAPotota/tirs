use core::fmt;

use byteorder::{ByteOrder, LE};
use strum::{EnumDiscriminants, FromRepr};
use thiserror::Error;

use crate::{
    util::{u16_from_bytes, u32_from_bytes},
    Calculator,
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

#[repr(u16)]
pub enum VariableAttribute {
    Size = 1,
    Kind = 2,
}

pub struct Variable {
    pub name: String,
    pub attributes: Vec<VariableAttribute>,
}

#[derive(Debug)]
pub enum Screenshot {
    Monochrome,             // 1 bit per pixel
    Grayscale,              // 4 bits per pixel (Nspire)
    Rgb(Box<[u16; 76800]>), // 16 bits per pixel (5 red, 6 green, 5 blue) (Nspire CX/84+CSE/83PCE/84+CE)
}

#[repr(u16)]
#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(name(ParameterKind))]
#[strum_discriminants(derive(FromRepr))]
pub enum Parameter {
    Name(String) = 0x02,
    ScreenWidth(u16) = 0x001e,
    ScreenHeight(u16) = 0x001f,
    ScreenContents(Screenshot) = 0x0022,
    Clock(u32) = 0x25,
}

impl Parameter {
    pub fn from_payload(kind: ParameterKind, payload: &[u8]) -> Self {
        match kind {
            ParameterKind::Name => Self::Name(String::from_utf8_lossy(payload).into_owned()),
            ParameterKind::ScreenWidth => Self::ScreenWidth(u16_from_bytes(&payload[0..2])),
            ParameterKind::ScreenHeight => Self::ScreenHeight(u16_from_bytes(&payload[0..2])),
            ParameterKind::ScreenContents => Self::ScreenContents(Screenshot::Rgb(Box::new({
                let a: Vec<u16> = payload.chunks_exact(2).map(LE::read_u16).collect();
                a.try_into().unwrap()
            }))),
            ParameterKind::Clock => Self::Clock(u32_from_bytes(&payload[0..4])),
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

pub fn request_directory_listing(handle: &mut Calculator) -> anyhow::Result<()> {
    todo!()
}
