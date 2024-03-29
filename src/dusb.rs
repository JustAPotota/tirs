use core::fmt;
use std::io;

use byteorder::{ByteOrder, ReadBytesExt, BE, LE};
use strum::{EnumDiscriminants, FromRepr};
use thiserror::Error;

use crate::util::{u16_from_bytes, u32_from_bytes};

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
#[derive(Debug, Clone, EnumDiscriminants)]
#[strum_discriminants(name(VariableAttributeKind))]
#[strum_discriminants(derive(FromRepr))]
pub enum VariableAttribute {
    Size(u32) = 0x01,
    Kind(u32) = 0x02,
    Archived(bool) = 0x03,
    AppVarSource(u32) = 0x05,
    Version(u8) = 0x08,
    Kind2(u32) = 0x11,
    Locked(bool) = 0x41,
}

impl VariableAttribute {
    pub fn from_payload(kind: VariableAttributeKind, mut payload: &[u8]) -> anyhow::Result<Self> {
        Ok(match kind {
            VariableAttributeKind::Size => Self::Size(payload.read_u32::<BE>()?),
            VariableAttributeKind::Kind => Self::Kind(payload.read_u32::<BE>()?),
            VariableAttributeKind::Archived => Self::Archived(payload.read_u8()? == 1), // Guessing that 1 == true, need to verify
            VariableAttributeKind::AppVarSource => Self::AppVarSource(payload.read_u32::<BE>()?),
            VariableAttributeKind::Version => Self::Version(payload.read_u8()?),
            VariableAttributeKind::Kind2 => Self::Kind2(payload.read_u32::<BE>()?),
            VariableAttributeKind::Locked => Self::Locked(payload.read_u8()? == 1), // Also guessing here
        })
    }

    pub fn into_payload(self) -> Vec<u8> {
        match self {
            Self::Size(size) => size.to_be_bytes().to_vec(),
            Self::Kind(kind) => kind.to_be_bytes().to_vec(),
            Self::Archived(archived) => {
                if archived {
                    vec![0; 1]
                } else {
                    vec![1; 1]
                }
            }
            Self::AppVarSource(source) => source.to_be_bytes().to_vec(),
            Self::Version(version) => version.to_be_bytes().to_vec(),
            Self::Kind2(kind) => kind.to_be_bytes().to_vec(),
            Self::Locked(locked) => {
                if locked {
                    vec![0; 1]
                } else {
                    vec![1; 1]
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub name: String,
    pub attributes: Vec<VariableAttribute>,
}

#[repr(u32)]
#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(name(VariableKind))]
#[strum_discriminants(derive(FromRepr))]
pub enum VariableContents {
    Image(Vec<u8>) = 0xf00e001a,
    String(String) = 0xf0070004,
    App(Vec<u8>) = 0xf00f0024,
}

impl VariableContents {
    pub fn from_payload(kind: VariableKind, mut payload: &[u8]) -> anyhow::Result<Self> {
        Ok(match kind {
            VariableKind::Image => Self::Image(payload.to_vec()),
            VariableKind::String => {
                let length = payload.read_u16::<LE>()?;
                let str = String::from_utf8_lossy(&payload[..length as usize]);
                Self::String(str.to_string())
            }
            VariableKind::App => Self::App(payload.to_vec()),
        })
    }

    pub fn into_payload(self) -> Vec<u8> {
        match self {
            Self::String(str) => {
                let mut payload = (str.len() as u16).to_le_bytes().to_vec();
                payload.extend_from_slice(str.as_bytes());
                payload
            }
            _ => todo!(),
        }
    }
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
    Name(String) = 0x0002,
    TotalAppPages(u64) = 0x0012,
    FreeAppPages(u64) = 0x0013,
    ScreenWidth(u16) = 0x001e,
    ScreenHeight(u16) = 0x001f,
    ScreenContents(Screenshot) = 0x0022,
    Clock(u32) = 0x25,
}

#[derive(Debug, Error)]
#[error("invalid parameter payload received")]
pub struct InvalidParameterPayload;

impl From<io::Error> for InvalidParameterPayload {
    fn from(_value: io::Error) -> Self {
        Self // TODO properly convert the error to be more specific
    }
}

impl Parameter {
    pub fn from_payload(
        kind: ParameterKind,
        mut payload: &[u8],
    ) -> Result<Self, InvalidParameterPayload> {
        Ok(match kind {
            ParameterKind::Name => Self::Name(String::from_utf8_lossy(payload).into_owned()),
            ParameterKind::TotalAppPages => Self::TotalAppPages(payload.read_u64::<BE>()?),
            ParameterKind::FreeAppPages => Self::FreeAppPages(payload.read_u64::<BE>()?),
            ParameterKind::ScreenWidth => Self::ScreenWidth(u16_from_bytes(&payload[0..2])),
            ParameterKind::ScreenHeight => Self::ScreenHeight(u16_from_bytes(&payload[0..2])),
            ParameterKind::ScreenContents => Self::ScreenContents(Screenshot::Rgb(Box::new({
                let a: Vec<u16> = payload.chunks_exact(2).map(LE::read_u16).collect();
                a.try_into().unwrap()
            }))),
            ParameterKind::Clock => Self::Clock(u32_from_bytes(&payload[0..4])),
        })
    }
}

#[derive(Error, Debug)]
pub struct UnknownParameterKindError(pub u16);
impl fmt::Display for UnknownParameterKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown parameter kind {}", self.0)
    }
}
