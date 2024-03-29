#![allow(clippy::unusual_byte_groupings)]

use std::{
    fs,
    io::{self, Read},
    path::Path,
    thread,
    time::Duration,
};

use anyhow::Context;
use dusb::{Mode, Variable, VariableAttribute, VariableAttributeKind, VariableKind};
use packet::raw::{self, RawPacket, RawPacketKind};
use rusb::{Device, DeviceHandle, GlobalContext};

use crate::{
    dusb::{Parameter, ParameterKind, Screenshot, VariableContents},
    packet::vtl::{self, VirtualPacket, VirtualPacketKind},
};

mod dusb;
mod packet;
mod util;

const TI_VENDOR: u16 = 0x0451;
const TI84_PLUS_SILVER: u16 = 0xe008;

pub struct Calculator {
    pub device: DeviceHandle<GlobalContext>,
    pub max_raw_packet_size: u32,
    pub timeout: Duration,
    buffer: Vec<u8>,
    read_endpoint: u8,
    pub debug_transfer: bool,
}

impl Calculator {
    pub fn new(device: DeviceHandle<GlobalContext>, timeout: Duration) -> anyhow::Result<Self> {
        let mut calculator = Self {
            device,
            max_raw_packet_size: 1019,
            timeout,
            buffer: Vec::new(),
            read_endpoint: 129,
            debug_transfer: false,
        };

        calculator.negotiate_packet_size(1019)?;

        Ok(calculator)
    }

    pub fn negotiate_packet_size(&mut self, max: u32) -> anyhow::Result<()> {
        RawPacket::RequestBufSize(max).send(self)?;
        let packet = RawPacket::receive_exact(RawPacketKind::BufSizeAlloc, self)?;

        match packet {
            RawPacket::RespondBufSize(mut size) => {
                println!("TI->PC: Responded with buffer size {size}");
                if size > 1018 {
                    println!(
                    "[The 83PCE/84+CE allocate more than they support. Clamping buffer size to 1018]"
                );
                    size = 1018;
                };
                self.max_raw_packet_size = size;
                Ok(())
            }
            packet => Err(raw::WrongPacketKind {
                expected: RawPacketKind::BufSizeAlloc,
                received: packet.kind(),
            }
            .into()),
        }
    }

    pub fn request_parameters(
        &mut self,
        parameters: &[ParameterKind],
    ) -> anyhow::Result<Vec<Parameter>> {
        self.negotiate_packet_size(self.max_raw_packet_size)?;

        println!("PC->TI: Requesting parameters {parameters:?}");

        VirtualPacket::ParameterRequest(parameters.to_vec()).send(self)?;

        Ok(match VirtualPacket::receive(self)? {
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

    pub fn request_directory(
        &mut self,
        attributes: &[VariableAttributeKind],
    ) -> anyhow::Result<Vec<Variable>> {
        VirtualPacket::DirectoryRequest(attributes.to_vec()).send(self)?;

        let mut variables = Vec::new();
        loop {
            let mut packet = VirtualPacket::receive(self)?;
            if let VirtualPacket::Wait(ms) = packet {
                println!("Waiting {ms}ms...");
                thread::sleep(Duration::from_millis(100));
                packet = VirtualPacket::receive(self)?;
            }

            match packet {
                VirtualPacket::VariableHeader(variable) => {
                    variables.push(variable);
                }
                VirtualPacket::EndOfTransmission => return Ok(variables),
                packet => {
                    return Err(vtl::WrongPacketKind {
                        expected: VirtualPacketKind::VariableHeader,
                        received: packet.into(),
                    }
                    .into())
                }
            }
        }
    }

    pub fn request_variable(&mut self, name: String) -> anyhow::Result<VariableContents> {
        let packet = VirtualPacket::RequestVariable(
            name,
            vec![
                VariableAttributeKind::Archived,
                VariableAttributeKind::Version,
                VariableAttributeKind::Size,
                VariableAttributeKind::Kind,
            ],
            vec![VariableAttribute::Kind2(0xf00e001a)],
        );
        packet.send(self)?;

        let kind = match VirtualPacket::receive(self)? {
            VirtualPacket::VariableHeader(variable) => VariableKind::from_repr(
                variable
                    .attributes
                    .iter()
                    .find_map(|attr| {
                        if let VariableAttribute::Kind(kind) = attr {
                            Some(*kind)
                        } else {
                            None
                        }
                    })
                    .unwrap(),
            )
            .unwrap(),

            VirtualPacket::Error(err) => return Err(err.into()),
            packet => {
                return Err(
                    vtl::WrongPacketKind::new(VirtualPacketKind::VariableHeader, packet).into(),
                );
            }
        };

        match VirtualPacket::receive(self)? {
            VirtualPacket::VariableContents(contents) => {
                Ok(VariableContents::from_payload(kind, &contents)?)
            }
            packet => {
                Err(vtl::WrongPacketKind::new(VirtualPacketKind::VariableContents, packet).into())
            }
        }
    }

    pub fn send_variable(
        &mut self,
        header: Variable,
        contents: VariableContents,
    ) -> anyhow::Result<()> {
        VirtualPacket::RequestToSend(header).send(self)?;
        VirtualPacket::VariableContents(contents.into_payload()).send(self)?;
        match VirtualPacket::receive(self)? {
            VirtualPacket::DataAcknowledge => {}
            packet => {
                return Err(
                    vtl::WrongPacketKind::new(VirtualPacketKind::DataAcknowledge, packet).into(),
                );
            }
        }
        VirtualPacket::EndOfTransmission.send(self)?;

        Ok(())
    }

    pub fn set_mode(&mut self, mode: Mode) -> anyhow::Result<()> {
        self.negotiate_packet_size(self.max_raw_packet_size)?;

        VirtualPacket::SetMode(mode).send(self)?;
        match VirtualPacket::receive(self)? {
            VirtualPacket::SetModeAcknowledge => Ok(()),
            packet => Err(vtl::WrongPacketKind {
                expected: VirtualPacketKind::SetModeAcknowledge,
                received: packet.into(),
            }
            .into()),
        }
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

impl Read for Calculator {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.debug_transfer {
            println!("Receiving {} bytes...", buf.len());
        }

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
            self.buffer.resize(1024, 0);
            let bytes_read =
                match self
                    .device
                    .read_bulk(self.read_endpoint, &mut self.buffer, self.timeout)
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

fn _take_screenshot<P>(calculator: &mut Calculator, output_path: P) -> anyhow::Result<()>
where
    P: AsRef<Path>,
{
    let parameters = calculator.request_parameters(&[
        ParameterKind::ScreenWidth,
        ParameterKind::ScreenHeight,
        ParameterKind::ScreenContents,
    ])?;

    let (mut width, mut height) = (0, 0);
    let mut pixels = Vec::new();
    for parameter in parameters {
        match parameter {
            Parameter::ScreenWidth(w) => width = w as u32,
            Parameter::ScreenHeight(h) => height = h as u32,
            Parameter::ScreenContents(Screenshot::Rgb(p)) => pixels = p.to_vec(),
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
    img.save(output_path)?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let calculator = find_calculator()?
        .with_context(|| "No calculator found")
        .unwrap();
    let descriptor = calculator.device_descriptor()?;
    let mut handle = calculator.open()?;
    println!(
        "Product string: {}\nVersion: {}",
        handle.read_product_string_ascii(&descriptor)?,
        descriptor.device_version()
    );

    handle.claim_interface(0)?;

    let mut calculator = Calculator::new(handle, Duration::from_secs(10))?;
    calculator.set_mode(Mode::Normal)?;

    let str = String::from("Test");
    calculator.send_variable(
        Variable {
            name: String::from("Str1"),
            attributes: vec![
                VariableAttribute::Size(str.len() as u32),
                VariableAttribute::Kind(0xf0070004),
                VariableAttribute::Version(0),
                VariableAttribute::Archived(false),
                VariableAttribute::Locked(false),
            ],
        },
        VariableContents::String(str),
    )?;

    // let var = calculator.request_variable("Str1".to_owned())?;
    // match var {
    //     VariableContents::Image(img) => fs::write("img.bin", img)?,
    //     VariableContents::String(s) => fs::write("str.txt", s)?,
    //     VariableContents::App(bytes) => fs::write("app.bin", bytes)?,
    // }

    // let variables = calculator.request_directory(&[
    //     VariableAttributeKind::Size,
    //     VariableAttributeKind::Kind,
    //     VariableAttributeKind::Version,
    //     VariableAttributeKind::Locked,
    //     VariableAttributeKind::Archived,
    // ])?;
    // let mut s = String::new();
    // for variable in variables {
    //     s.push_str(&format!("{variable:02x?}\n"));
    // }
    // fs::write("variables.txt", s)?;

    Ok(())
}
