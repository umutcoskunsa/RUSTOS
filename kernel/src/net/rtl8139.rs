use x86_64::instructions::port::Port;

static mut IO_BASE: u16 = 0;

pub fn init() {
    crate::serial_println!("RTL8139: Looking for device...");
    
    // 1. Find device on PCI bus
    let mut found_bus = None;
    let mut found_dev = None;
    
    for bus in 0..=255 {
        for device in 0..=31 {
            let id_reg = crate::pci::read_config_32(bus, device, 0, 0);
            let vendor = (id_reg & 0xFFFF) as u16;
            let dev_id = (id_reg >> 16) as u16;
            if vendor == 0x10EC && dev_id == 0x8139 {
                found_bus = Some(bus);
                found_dev = Some(device);
                break;
            }
        }
    }
    
    if let (Some(bus), Some(device)) = (found_bus, found_dev) {
        crate::serial_println!("RTL8139: Found at Bus {} Device {}", bus, device);
        
        // 2. Read BAR0 to get I/O Port base address
        let bar0 = crate::pci::read_config_32(bus, device, 0, 0x10);
        if bar0 & 1 == 0 {
            crate::serial_println!("RTL8139: ERROR - BAR0 is not an I/O space. BAR0={:#x}", bar0);
            return;
        }
        
        let io_base = (bar0 & !3) as u16;
        unsafe { IO_BASE = io_base; }
        crate::serial_println!("RTL8139: I/O Base Port = {:#06x}", io_base);
        
        // 3. Enable PCI Bus Mastering
        let mut cmd_reg = crate::pci::read_config_32(bus, device, 0, 0x04);
        cmd_reg |= 1 << 2; // Enable Bus Master
        crate::pci::write_config_32(bus, device, 0, 0x04, cmd_reg);
        crate::serial_println!("RTL8139: PCI Bus Mastering Enabled.");
        
        // 4. Power on the device
        // Write 0x00 to CONFIG_1 (offset 0x52)
        let mut config_1: Port<u8> = Port::new(io_base + 0x52);
        unsafe { config_1.write(0x00) };
        crate::serial_println!("RTL8139: Powered ON.");
        
        // 5. Software Reset
        // Write 0x10 to Command Register (offset 0x37)
        let mut cmd_port: Port<u8> = Port::new(io_base + 0x37);
        unsafe { cmd_port.write(0x10) };
        
        // Wait for reset to complete
        crate::serial_println!("RTL8139: Resetting...");
        while unsafe { cmd_port.read() } & 0x10 != 0 {
            core::hint::spin_loop();
        }
        crate::serial_println!("RTL8139: Reset Complete.");
        
        // Let's print the MAC address
        let mut mac_port0: Port<u32> = Port::new(io_base + 0x00);
        let mut mac_port4: Port<u16> = Port::new(io_base + 0x04);
        let mac0 = unsafe { mac_port0.read() };
        let mac4 = unsafe { mac_port4.read() };
        
        crate::serial_println!("RTL8139 MAC Address: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            (mac0 >> 0) & 0xFF, (mac0 >> 8) & 0xFF, (mac0 >> 16) & 0xFF, (mac0 >> 24) & 0xFF,
            (mac4 >> 0) & 0xFF, (mac4 >> 8) & 0xFF
        );
        
        // 6. Allocate Receive (RX) Buffer (12KB contiguous physical memory)
        // RTL8139 RCR supports 8K + 16 bytes. We allocate 3 frames (12KB).
        let rx_phys_addr = {
            use x86_64::structures::paging::FrameAllocator;
            let mut allocator = crate::memory::GLOBAL_FRAME_ALLOCATOR.lock();
            let alloc_ref = allocator.as_mut().expect("No Frame Allocator");
            
            let frame1 = alloc_ref.allocate_frame().unwrap();
            let _frame2 = alloc_ref.allocate_frame().unwrap(); // Assuming contiguous
            let _frame3 = alloc_ref.allocate_frame().unwrap(); // Assuming contiguous
            
            frame1.start_address().as_u64()
        };
        
        unsafe {
            RX_BUFFER_PHYS = rx_phys_addr;
            RX_BUFFER_VIRT = rx_phys_addr;
            CAPR = 0;
        }
        
        crate::serial_println!("RTL8139: RX Buffer Physical Address = {:#x}", rx_phys_addr);
        
        // Write the physical address to RBSTART (Receive Buffer Start Address)
        let mut rbstart_port: Port<u32> = Port::new(io_base + 0x30);
        unsafe { rbstart_port.write(rx_phys_addr as u32) };
        
        // 7. Setup Interrupt Mask Register (IMR)
        // Enable Receive OK (0x01) and Transmit OK (0x04)
        let mut imr_port: Port<u16> = Port::new(io_base + 0x3C);
        unsafe { imr_port.write(0x0005) };
        
        // 8. Setup Receive Configuration Register (RCR)
        // Accept Broadcast | Multicast | Physical Match | Promiscuous = 0x0F
        // WRAP bit = 0x80
        let mut rcr_port: Port<u32> = Port::new(io_base + 0x44);
        unsafe { rcr_port.write(0x8F) };
        
        // 9. Enable Receive and Transmit
        // RE (0x08) | TE (0x04) = 0x0C
        unsafe { cmd_port.write(0x0C) };
        
        // Read Interrupt Line
        let intr_info = crate::pci::read_config_32(bus, device, 0, 0x3C);
        let intr_line = (intr_info & 0xFF) as u8;
        crate::serial_println!("RTL8139: Hardware Interrupt Line (IRQ) = {}", intr_line);
        
        // Route the IRQ via IOAPIC
        crate::apic::route_irq(intr_line, crate::interrupts::InterruptIndex::Network as u8);
        
        setup_tx_buffer();
        crate::serial_println!("RTL8139: Initialized and ready to receive/transmit!");
        
    } else {
        crate::serial_println!("RTL8139: NOT FOUND on PCI Bus.");
    }
}

pub fn handle_interrupt() {
    let io_base = unsafe { IO_BASE };
    if io_base == 0 { return; }
    
    let mut isr_port: Port<u16> = Port::new(io_base + 0x3E);
    let status = unsafe { isr_port.read() };
    
    // Check if it's a Receive OK (ROK) interrupt
    if status & 0x01 != 0 {
        receive_packets();
        // Clear the ROK bit by writing it back
        unsafe { isr_port.write(0x01) };
    }
    
    // Check Transmit OK (TOK)
    if status & 0x04 != 0 {
        crate::serial_println!("RTL8139: Transmitted Packet! (ISR: {:#x})", status);
        // Clear TOK bit
        unsafe { isr_port.write(0x04) };
    }
}

static mut TX_BUFFER_PHYS: u64 = 0;
static mut TX_BUFFER_VIRT: u64 = 0;
static mut TX_DESC_INDEX: u8 = 0;

static mut RX_BUFFER_PHYS: u64 = 0;
static mut RX_BUFFER_VIRT: u64 = 0;
static mut CAPR: u16 = 0;

pub fn setup_tx_buffer() {
    // Allocate 1 frame for TX (4KB)
    use x86_64::structures::paging::FrameAllocator;
    let mut allocator = crate::memory::GLOBAL_FRAME_ALLOCATOR.lock();
    let alloc_ref = allocator.as_mut().expect("No Frame Allocator");
    let frame = alloc_ref.allocate_frame().unwrap();
    
    unsafe {
        TX_BUFFER_PHYS = frame.start_address().as_u64();
        // Since we identity map the first few MBs, virtual = physical
        TX_BUFFER_VIRT = TX_BUFFER_PHYS; 
    }
    crate::serial_println!("RTL8139: TX Buffer Physical Address = {:#x}", unsafe { TX_BUFFER_PHYS });
}

pub fn send_packet(data: &[u8]) {
    let io_base = unsafe { IO_BASE };
    if io_base == 0 { return; }
    
    unsafe {
        let dest = core::slice::from_raw_parts_mut(TX_BUFFER_VIRT as *mut u8, data.len());
        dest.copy_from_slice(data);
        
        // Find which descriptor to use (0 to 3)
        let desc = TX_DESC_INDEX;
        TX_DESC_INDEX = (TX_DESC_INDEX + 1) % 4;
        
        // Write physical address to TSAD
        let mut tsad_port: Port<u32> = Port::new(io_base + 0x20 + (desc as u16 * 4));
        tsad_port.write(TX_BUFFER_PHYS as u32);
        
        // Write length to TSD to trigger transmission
        let mut tsd_port: Port<u32> = Port::new(io_base + 0x10 + (desc as u16 * 4));
        tsd_port.write(data.len() as u32);
        
        crate::serial_println!("RTL8139: Sending packet of {} bytes via Descriptor {}", data.len(), desc);
    }
}

fn receive_packets() {
    let io_base = unsafe { IO_BASE };
    let mut cmd_port: Port<u8> = Port::new(io_base + 0x37);
    let mut capr_port: Port<u16> = Port::new(io_base + 0x38);
    
    // As long as the buffer is not empty (Command register bit 0 is 0)
    while unsafe { cmd_port.read() } & 0x01 == 0 {
        unsafe {
            let offset = CAPR as usize;
            let rx_ptr = (RX_BUFFER_VIRT as usize + offset) as *const u8;
            
            // Read 16-bit status and 16-bit length
            let status = core::ptr::read_unaligned(rx_ptr as *const u16);
            let length = core::ptr::read_unaligned(rx_ptr.add(2) as *const u16);
            
            if status & 0x01 != 0 {
                // Packet is OK (ROK)
                // The actual payload is length - 4 (minus CRC)
                let packet_len = length as usize - 4;
                let packet_data = core::slice::from_raw_parts(rx_ptr.add(4), packet_len);
                
                crate::serial_println!("RTL8139: Received packet of {} bytes!", packet_len);
                
                // Pass packet_data to Ethernet Layer
                crate::net::ethernet::handle_packet(packet_data);
            }
            
            // Update CAPR: packet size + header (4 bytes) + 4-byte alignment
            CAPR = (CAPR + length + 4 + 3) & !3;
            // RTL8139 wrap around (8K buffer = 8192)
            if CAPR >= 8192 {
                CAPR -= 8192;
            }
            
            // Write CAPR back to card (minus 16 per RTL8139 specs to avoid overflow bugs)
            capr_port.write(CAPR.wrapping_sub(16));
        }
    }
}
