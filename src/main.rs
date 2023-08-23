use std::{
    io::{self, Read},
    thread,
    time::Duration,
};

use rusb::{Device, DeviceHandle, GlobalContext};

mod dusb;
mod packet;

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

        if buf.len() > self.buffer.len() && !self.buffer.is_empty() {
            // TODO read more bytes
            return Err(io::ErrorKind::Other.into());
        }
        if self.buffer.is_empty() {
            self.buffer.resize(self.max_raw_packet_size as usize, 0);
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
        let (left, right) = self.buffer.split_at(bytes_requested);

        buf.copy_from_slice(left);
        self.buffer = right.to_owned();

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
    let calculator = {
        let mut calc = find_calculator()?;
        while calc.is_none() {
            thread::sleep(Duration::from_secs_f64(0.5));
            calc = find_calculator()?;
            println!("No calc");
        }
        calc.unwrap()
    };
    let descriptor = calculator.device_descriptor()?;
    let active_config = calculator.active_config_descriptor()?;
    let mut handle = calculator.open()?;
    println!(
        "Product string: {}\nVersion: {}",
        handle.read_product_string_ascii(&descriptor)?,
        descriptor.device_version()
    );

    println!("{}", handle.active_configuration()?);

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

    let mut handle = CalcHandle::new(handle, Duration::from_secs(5))?;
    dusb::cmd_send_mode_set(&mut handle, dusb::MODE_NORMAL)?;

    Ok(())
}
