use std::time::{Duration, Instant};

use rusb::{DeviceHandle, UsbContext};

use crate::CalcHandle;

// fn read_(handle: &mut CalcHandle, data: &mut [u8]) -> anyhow::Result<()> {
//     if handle.bytes_read == 0 {
//         handle.bytes_read =
//             handle
//                 .device
//                 .read_bulk(129, &mut handle.buffer, Duration::from_secs(5))?;
//     }
//     data.copy_from_slice(&handle.buffer[0..data.len()]);
//     handle.buffer.rotate_left(data.len());
//     handle.bytes_read -= data.len();

//     Ok(())
// }

// pub fn cable_read(handle: &mut CalcHandle, buf: &mut [u8], size: usize) -> anyhow::Result<()> {
//     println!("Reading {} bytes...", buf.len());
//     if buf.len() <= 64 {
//         read_(handle, buf)?;
//         println!("{buf:02x?}");
//         //handle.device.read_bulk(129, buf, Duration::from_secs(5))?;
//     } else {
//         let reads_required = buf.len() / 64;
//         let extra_bytes = buf.len() % 64;
//         println!("More than 64 requested, splitting it into {reads_required} chunks (+ {extra_bytes} bytes)");
//         for i in 0..reads_required {
//             let i = i * 64;
//             handle
//                 .device
//                 .read_bulk(129, &mut buf[i..i + 64], Duration::from_secs(5))?;
//         }

//         if extra_bytes > 0 {
//             handle.device.read_bulk(
//                 129,
//                 &mut buf[reads_required * 64..reads_required * 64 + extra_bytes],
//                 Duration::from_secs(5),
//             )?;
//         }
//     }
//     Ok(())
// }

pub fn write(handle: &CalcHandle, data: &[u8], size: usize) -> anyhow::Result<()> {
    println!("Writing {} bytes...", data.len());
    println!("{data:02x?}");
    handle.device.write_bulk(2, data, Duration::from_secs(5))?;
    Ok(())
}
