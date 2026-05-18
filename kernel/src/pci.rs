use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

pub fn read_config_32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let address = 0x80000000u32 
                | ((bus as u32) << 16) 
                | ((device as u32) << 11) 
                | ((func as u32) << 8) 
                | (offset as u32 & 0xFC);
                
    let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
    let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
    
    unsafe {
        addr_port.write(address);
        data_port.read()
    }
}

pub fn write_config_32(bus: u8, device: u8, func: u8, offset: u8, value: u32) {
    let address = 0x80000000u32 
                | ((bus as u32) << 16) 
                | ((device as u32) << 11) 
                | ((func as u32) << 8) 
                | (offset as u32 & 0xFC);
                
    let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
    let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
    
    unsafe {
        addr_port.write(address);
        data_port.write(value);
    }
}

pub fn scan_bus() {
    crate::serial_println!("Scanning PCI Bus...");
    crate::println!("--- PCI Devices Found ---");
    for bus in 0..=255 {
        for device in 0..=31 {
            // Read vendor ID (offset 0). If it's 0xFFFF, the device doesn't exist.
            let id_reg = read_config_32(bus, device, 0, 0);
            let vendor = (id_reg & 0xFFFF) as u16;
            let dev_id = (id_reg >> 16) as u16;
            
            if vendor != 0xFFFF {
                let class_reg = read_config_32(bus, device, 0, 0x08);
                let class = (class_reg >> 24) as u8;
                let subclass = (class_reg >> 16) as u8;
                
                crate::serial_println!("PCI Device Found: Bus {}, Device {} - Vendor: {:#06x}, Device: {:#06x}, Class: {:#04x}, Subclass: {:#04x}", 
                    bus, device, vendor, dev_id, class, subclass);
                crate::println!("Bus {} Dev {}: Vendor={:#06x} Dev={:#06x}", bus, device, vendor, dev_id);
            }
        }
    }
}
