use nom::{
    multi::{count, length_value},
    number::complete::{be_u16, be_u8},
    IResult,
};

use crate::dusb::Parameter;

use super::vtl::VirtualPacket;

pub fn parameter_size(input: &[u8]) -> IResult<&[u8], u32> {
    let (input, size) = be_u16(input)?;
    let size = if size == 0 {
        153600
    } else {
        size as u32
    };
    Ok((input, size))
}

// pub fn parameter_data(input: &[u8]) -> IResult<&[u8], >

pub fn parameter(input: &[u8]) -> IResult<&[u8], Option<Parameter>> {
    let (input, kind) = be_u16(input)?;
    let (input, is_invalid) = be_u8(input)?;
    if is_invalid > 0 {
        Ok((input, None))
    } else {
        length_value(parameter_size, )(input)
    }
}

pub fn parameter_response(input: &[u8]) -> IResult<&[u8], VirtualPacket> {
    let (input, amount) = be_u16(input)?;
    let (input, parameters) = count(parameter, amount as usize)?;
}
