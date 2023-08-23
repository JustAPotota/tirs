use core::fmt;

use thiserror::Error;

#[repr(u16)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum VirtualPacketKind {
    Ping = 1,
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
            1 => Ok(Self::Ping),
            n => Err(UnknownPacketKindError(n)),
        }
    }
}
