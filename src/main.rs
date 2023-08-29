use std::{
    io::{self, Read},
    time::Duration,
};

use anyhow::Context;
use image::GenericImage;
use rusb::{Device, DeviceHandle, GlobalContext};

use crate::dusb::{Parameter, ParameterKind, Screenshot};

mod dusb;
mod packet;
mod util;

const TI_VENDOR: u16 = 0x0451;
const TI84_PLUS_SILVER: u16 = 0xe008;

pub struct CalcHandle {
    pub device: DeviceHandle<GlobalContext>,
    pub max_raw_packet_size: u32,
    pub timeout: Duration,
    buffer: Vec<u8>,
    read_endpoint: u8,
    pub debug_transfer: bool,
}

impl CalcHandle {
    pub fn new(device: DeviceHandle<GlobalContext>, timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            device,
            max_raw_packet_size: 64,
            timeout,
            buffer: Vec::new(),
            read_endpoint: 129,
            debug_transfer: false,
        })
    }

    pub fn send(&self, bytes: &[u8]) -> anyhow::Result<()> {
        if self.debug_transfer {
            println!("Sending {} bytes...", bytes.len());
            println!("{bytes:02x?}");
        }

        self.device.write_bulk(2, bytes, Duration::from_secs(5))?;
        Ok(())
    }
}

impl Read for CalcHandle {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.debug_transfer {
            println!("Receiving {} bytes...", buf.len());
        }

        // if buf.len() > self.max_raw_packet_size as usize {
        //     for i in 0..
        // }

        if buf.len() > self.buffer.len() && !self.buffer.is_empty() {
            let bytes_read = self.buffer.len();
            buf[0..bytes_read].copy_from_slice(&self.buffer);
            self.buffer.clear();

            return Ok(bytes_read);
        }

        if buf.len() > self.max_raw_packet_size as usize {
            return self.read(&mut buf[0..self.max_raw_packet_size as usize]);
        }

        if self.buffer.is_empty() {
            //self.buffer.resize(self.max_raw_packet_size as usize, 0);
            self.buffer.resize(1024, 0);
            let bytes_read = match self
                    .device
                    .read_bulk(self.read_endpoint, &mut self.buffer, self.timeout) // Overflow
                {
                    Ok(bytes) => bytes,
                    Err(err) => return Err(io::Error::new(io::ErrorKind::Other, err)),
                };
            self.buffer.truncate(bytes_read);
        }

        let bytes_requested = buf.len();
        let (requested, leftover) = self.buffer.split_at(bytes_requested);

        buf.copy_from_slice(requested);
        self.buffer = leftover.to_owned();

        if self.debug_transfer {
            println!("{buf:02x?}");
        }

        Ok(bytes_requested)
    }
}

fn find_calculator() -> anyhow::Result<Option<Device<GlobalContext>>> {
    Ok(rusb::devices()?.iter().find(|device| {
        let descriptor = device.device_descriptor().unwrap();
        descriptor.vendor_id() == TI_VENDOR && descriptor.product_id() == TI84_PLUS_SILVER
    }))
}

fn main() -> anyhow::Result<()> {
    let calculator = find_calculator()?
        .with_context(|| "No calculator found")
        .unwrap();
    let descriptor = calculator.device_descriptor()?;
    let active_config = calculator.active_config_descriptor()?;
    let mut handle = calculator.open()?;
    println!(
        "Product string: {}\nVersion: {}",
        handle.read_product_string_ascii(&descriptor)?,
        descriptor.device_version()
    );

    let interface = active_config.interfaces().next().unwrap();
    let max_packet_size = interface
        .descriptors()
        .next()
        .unwrap()
        .endpoint_descriptors()
        .next()
        .unwrap()
        .max_packet_size();

    println!("max_ps: {max_packet_size}");
    handle.claim_interface(0)?;

    let mut handle = CalcHandle::new(handle, Duration::from_secs(10))?;
    dusb::set_mode(&mut handle, dusb::Mode::Normal)?;
    let parameters = dusb::request_parameters(
        &mut handle,
        &[
            ParameterKind::ScreenWidth,
            ParameterKind::ScreenHeight,
            ParameterKind::ScreenContents,
        ],
    )?;

    let (mut width, mut height) = (0, 0);
    let mut pixels = Vec::new();
    for parameter in parameters {
        match parameter {
            Parameter::ScreenWidth(w) => width = w as u32,
            Parameter::ScreenHeight(h) => height = h as u32,
            Parameter::ScreenContents(screenshot) => match screenshot {
                Screenshot::Rgb(p) => pixels = p.to_vec(),
                _ => {}
            },
            _ => {}
        }
    }

    let mut img = image::RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = pixels[(y * width + x) as usize];
            let r = (pixel & 0b11111_000000_00000) >> 11;
            let r = r as f32 / 31.0 * 255.0;

            let g = (pixel & 0b00000_111111_00000) >> 5;
            let g = g as f32 / 63.0 * 255.0;

            let b = pixel & 0b00000_000000_11111;
            let b = b as f32 / 31.0 * 255.0;

            img.put_pixel(x, y, image::Rgb([r as u8, g as u8, b as u8]));
        }
    }
    img.save("screenshot.png")?;
    Ok(())
}
