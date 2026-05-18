#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Ipv4Header {
    pub version_ihl: u8,
    pub dscp_ecn: u8,
    pub total_length: u16,
    pub identification: u16,
    pub flags_fragment_offset: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub header_checksum: u16,
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
}

pub fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i < data.len() {
        let word = if i + 1 < data.len() {
            ((data[i] as u32) << 8) | (data[i+1] as u32)
        } else {
            (data[i] as u32) << 8
        };
        sum += word;
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn handle_packet(data: &[u8]) {
    if data.len() < 20 { return; }
    
    let header_ptr = data.as_ptr() as *const Ipv4Header;
    let header = unsafe { &*header_ptr };
    
    crate::serial_println!("IPv4: Received Packet from {}.{}.{}.{} to {}.{}.{}.{} (Proto: {})",
        header.source_ip[0], header.source_ip[1], header.source_ip[2], header.source_ip[3],
        header.dest_ip[0], header.dest_ip[1], header.dest_ip[2], header.dest_ip[3],
        header.protocol
    );
    
    if header.protocol == 17 {
        crate::serial_println!("IPv4: Handing off to UDP...");
        crate::net::udp::handle_packet(&data[20..]);
    } else {
        crate::serial_println!("IPv4: Protocol {} not supported yet.", header.protocol);
    }
}
