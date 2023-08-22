use std::{thread, time::Duration};

use rusb::{
    ConfigDescriptor, Device, DeviceHandle, EndpointDescriptor, GlobalContext, InterfaceDescriptor,
    UsbContext,
};

mod dusb;
mod ticables;

const TI_VENDOR: u16 = 0x0451;
const TI84_PLUS_SILVER: u16 = 0xe008;

pub struct CalcHandle {
    pub device: DeviceHandle<GlobalContext>,
    pub max_raw_packet_size: u32,
    pub bytes_read: usize,
}

fn find_calculator() -> anyhow::Result<Option<Device<GlobalContext>>> {
    Ok(rusb::devices()?.iter().find(|device| {
        let descriptor = device.device_descriptor().unwrap();
        descriptor.vendor_id() == TI_VENDOR && descriptor.product_id() == TI84_PLUS_SILVER
    }))
}
// slv_put = ticables_send
// slv_get = ticables_recv

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
    let mut handle = calculator.open()?;
    println!(
        "Product string: {}\nVersion: {}",
        handle.read_product_string_ascii(&descriptor)?,
        descriptor.device_version()
    );

    println!("{}", handle.active_configuration()?);

    for i in 0..descriptor.num_configurations() {
        let config_desc = match calculator.config_descriptor(i) {
            Ok(c) => c,
            Err(_) => continue,
        };
        //print_config(&config_desc);
        for interface in config_desc.interfaces() {
            for interface_desc in interface.descriptors() {
                //print_interface(&interface_desc);
                for (i, endpoint_desc) in interface_desc.endpoint_descriptors().enumerate() {
                    // println!(
                    //     "interface: {}, endpoint addr: {}, endpoint #: {i}",
                    //     interface.number(),
                    //     endpoint_desc.address()
                    // );
                    //print_endpoint(&endpoint_desc);
                }
            }
        }
    }
    handle.set_active_configuration(1)?;
    handle.claim_interface(0)?;
    handle.set_alternate_setting(0, 0)?;
    let config = calculator.config_descriptor(1)?;
    //{ 3, 1, 0, 0, 0x07d0 }

    // let mut buf = vec![0; 256];
    // handle.read_interrupt(129, &mut buf, Duration::from_secs(10))?;

    // buf[0] = target
    // buf[1] = cmd = 68
    // buf[2] = 0
    // buf[3] = 0
    //handle.write_bulk(2, &[0, 68, 0, 0], Duration::from_secs(10))?;

    let mut handle = CalcHandle {
        device: handle,
        max_raw_packet_size: 0,
        bytes_read: 0,
    };
    dusb::cmd_send_mode_set(&mut handle, dusb::MODE_NORMAL)?;

    Ok(())
}

fn print_config(config_desc: &ConfigDescriptor) {
    println!("  Config Descriptor:");
    println!(
        "    bNumInterfaces       {:3}",
        config_desc.num_interfaces()
    );
    println!("    bConfigurationValue  {:3}", config_desc.number());
    // println!(
    //     "    iConfiguration       {:3} {}",
    //     config_desc.description_string_index().unwrap_or(0),
    //     handle.as_mut().map_or(String::new(), |h| h
    //         .handle
    //         .read_configuration_string(h.language, config_desc, h.timeout)
    //         .unwrap_or_default())
    // );
    println!("    bmAttributes:");
    println!("      Self Powered     {:>5}", config_desc.self_powered());
    println!("      Remote Wakeup    {:>5}", config_desc.remote_wakeup());
    println!("    bMaxPower           {:4}mW", config_desc.max_power());

    if !config_desc.extra().is_empty() {
        println!("    {:?}", config_desc.extra());
    } else {
        println!("    no extra data");
    }
}

fn print_interface(
    interface_desc: &InterfaceDescriptor,
    //handle: &mut Option<UsbDevice<T>>,
) {
    println!("    Interface Descriptor:");
    println!(
        "      bInterfaceNumber     {:3}",
        interface_desc.interface_number()
    );
    println!(
        "      bAlternateSetting    {:3}",
        interface_desc.setting_number()
    );
    println!(
        "      bNumEndpoints        {:3}",
        interface_desc.num_endpoints()
    );
    println!(
        "      bInterfaceClass     {:#04x}",
        interface_desc.class_code()
    );
    println!(
        "      bInterfaceSubClass  {:#04x}",
        interface_desc.sub_class_code()
    );
    println!(
        "      bInterfaceProtocol  {:#04x}",
        interface_desc.protocol_code()
    );
    // println!(
    //     "      iInterface           {:3} {}",
    //     interface_desc.description_string_index().unwrap_or(0),
    //     handle.as_mut().map_or(String::new(), |h| h
    //         .handle
    //         .read_interface_string(h.language, interface_desc, h.timeout)
    //         .unwrap_or_default())
    // );

    if interface_desc.extra().is_empty() {
        println!("    {:?}", interface_desc.extra());
    } else {
        println!("    no extra data");
    }
}

fn print_endpoint(endpoint_desc: &EndpointDescriptor) {
    println!("      Endpoint Descriptor:");
    println!(
        "        bEndpointAddress    {:#04x} EP {} {:?}",
        endpoint_desc.address(),
        endpoint_desc.number(),
        endpoint_desc.direction()
    );
    println!("        bmAttributes:");
    println!(
        "          Transfer Type          {:?}",
        endpoint_desc.transfer_type()
    );
    println!(
        "          Synch Type             {:?}",
        endpoint_desc.sync_type()
    );
    println!(
        "          Usage Type             {:?}",
        endpoint_desc.usage_type()
    );
    println!(
        "        wMaxPacketSize    {:#06x}",
        endpoint_desc.max_packet_size()
    );
    println!(
        "        bInterval            {:3}",
        endpoint_desc.interval()
    );
}
