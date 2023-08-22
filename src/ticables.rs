use std::time::{Duration, Instant};

use rusb::{DeviceHandle, UsbContext};

use crate::CalcHandle;

fn read_(handle: &mut CalcHandle, data: &mut [u8]) -> anyhow::Result<()> {
    if handle.bytes_read == 0 {
        let mut buf = vec![0; 0x40];
        handle.bytes_read = handle
            .device
            .read_bulk(129, &mut buf, Duration::from_secs(5))?;
    }
    Ok(())
}

pub fn cable_read(handle: &CalcHandle, buf: &mut [u8], size: usize) -> anyhow::Result<()> {
    println!("Reading {} bytes...", buf.len());
    if buf.len() <= 64 {
        handle.device.read_bulk(129, buf, Duration::from_secs(5))?;
    } else {
        let reads_required = buf.len() / 64;
        let extra_bytes = buf.len() % 64;
        println!("More than 64 requested, splitting it into {reads_required} chunks (+ {extra_bytes} bytes)");
        for i in 0..reads_required {
            let i = i * 64;
            handle
                .device
                .read_bulk(129, &mut buf[i..i + 64], Duration::from_secs(5))?;
        }

        if extra_bytes > 0 {
            handle.device.read_bulk(
                129,
                &mut buf[reads_required * 64..reads_required * 64 + extra_bytes],
                Duration::from_secs(5),
            )?;
        }
    }
    Ok(())
}

pub fn write(handle: &CalcHandle, data: &[u8], size: usize) -> anyhow::Result<()> {
    println!("Writing {} bytes...", data.len());
    println!("{data:x?}");
    handle.device.write_bulk(2, data, Duration::from_secs(10))?;
    Ok(())
}
