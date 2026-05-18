use core::ptr::write_unaligned;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ArpHeader {
    pub htype: u16,     // Hardware type (1 for Ethernet)
    pub ptype: u16,     // Protocol type (0x0800 for IPv4)
    pub hlen: u8,       // Hardware length (6 for MAC)
    pub plen: u8,       // Protocol length (4 for IPv4)
    pub oper: u16,      // Operation (1 for Request, 2 for Reply)
    pub sha: [u8; 6],   // Sender MAC
    pub spa: [u8; 4],   // Sender IP
    pub tha: [u8; 6],   // Target MAC
    pub tpa: [u8; 4],   // Target IP
}

pub fn send_request(target_ip: [u8; 4]) {
    let mut packet = [0u8; 14 + 28]; // Ethernet header (14) + ARP (28)
    
    let dest_mac = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
    let src_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]; // our MAC
    
    // Ethernet Header
    for i in 0..6 { packet[i] = dest_mac[i]; }
    for i in 0..6 { packet[6+i] = src_mac[i]; }
    packet[12] = 0x08; packet[13] = 0x06; // ARP EtherType
    
    // ARP Header
    let arp_ptr = packet[14..].as_mut_ptr() as *mut ArpHeader;
    unsafe {
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).htype), 0x0100); // 1 in big endian
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).ptype), 0x0008); // 0x0800 in big endian
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).hlen), 6);
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).plen), 4);
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).oper), 0x0100); // 1 in big endian (Request)
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).sha), src_mac);
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).spa), [10, 0, 2, 15]); // Our IP (QEMU default guest IP)
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).tha), [0, 0, 0, 0, 0, 0]);
        write_unaligned(core::ptr::addr_of_mut!((*arp_ptr).tpa), target_ip);
    }
    
    crate::net::rtl8139::send_packet(&packet);
    crate::serial_println!("ARP: Broadcasted Request for IP {}.{}.{}.{}", 
        target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
}

pub fn handle_packet(data: &[u8]) {
    if data.len() < 28 { return; }
    
    let arp_ptr = data.as_ptr() as *const ArpHeader;
    let arp = unsafe { &*arp_ptr };
    
    let oper = u16::from_be(arp.oper);
    if oper == 2 {
        crate::serial_println!("ARP: Received Reply! IP {}.{}.{}.{} has MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            arp.spa[0], arp.spa[1], arp.spa[2], arp.spa[3],
            arp.sha[0], arp.sha[1], arp.sha[2], arp.sha[3], arp.sha[4], arp.sha[5]
        );
    } else if oper == 1 {
        crate::serial_println!("ARP: Received Request for IP {}.{}.{}.{}", arp.tpa[0], arp.tpa[1], arp.tpa[2], arp.tpa[3]);
    }
}
