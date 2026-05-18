use alloc::vec::Vec;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct UdpHeader {
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
    pub checksum: u16,
}

pub fn handle_packet(data: &[u8]) {
    if data.len() < 8 { return; }
    
    let header_ptr = data.as_ptr() as *const UdpHeader;
    let header = unsafe { &*header_ptr };
    
    let src_port = u16::from_be(header.source_port);
    let dst_port = u16::from_be(header.dest_port);
    let len = u16::from_be(header.length);
    
    crate::serial_println!("UDP: Received {} bytes from Port {} to Port {}", len, src_port, dst_port);
    
    if data.len() > 8 {
        let payload = &data[8..];
        if let Ok(msg) = core::str::from_utf8(payload) {
            crate::serial_println!("UDP Payload: {}", msg);
            crate::println!("NET: Received UDP Message -> {}", msg);
        } else {
            crate::serial_println!("UDP Payload: <Binary Data>");
            crate::println!("NET: Received UDP Binary Data.");
        }
    }
}

pub fn send(dest_mac: [u8; 6], dest_ip: [u8; 4], dest_port: u16, src_port: u16, payload: &[u8]) {
    let mut packet = alloc::vec::Vec::new();
    packet.resize(14 + 20 + 8 + payload.len(), 0);
    
    // 1. Ethernet Header (14 bytes)
    let src_mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]; // our MAC
    for i in 0..6 { packet[i] = dest_mac[i]; }
    for i in 0..6 { packet[6+i] = src_mac[i]; }
    packet[12] = 0x08; packet[13] = 0x00; // IPv4 EtherType
    
    // 2. IPv4 Header (20 bytes)
    let ip_len = (20 + 8 + payload.len()) as u16;
    packet[14] = 0x45; // Version 4, IHL 5
    packet[15] = 0; // DSCP/ECN
    packet[16] = (ip_len >> 8) as u8;
    packet[17] = (ip_len & 0xFF) as u8;
    packet[18] = 0; packet[19] = 0; // ID
    packet[20] = 0x40; packet[21] = 0; // Flags (Don't fragment)
    packet[22] = 64; // TTL
    packet[23] = 17; // Protocol (UDP)
    packet[24] = 0; packet[25] = 0; // Initial Checksum
    packet[26] = 10; packet[27] = 0; packet[28] = 2; packet[29] = 15; // Source IP
    for i in 0..4 { packet[30+i] = dest_ip[i]; }
    
    // Calculate IPv4 Checksum
    let ip_checksum = crate::net::ipv4::calculate_checksum(&packet[14..34]);
    packet[24] = (ip_checksum >> 8) as u8;
    packet[25] = (ip_checksum & 0xFF) as u8;
    
    // 3. UDP Header (8 bytes)
    let udp_len = (8 + payload.len()) as u16;
    packet[34] = (src_port >> 8) as u8; packet[35] = (src_port & 0xFF) as u8;
    packet[36] = (dest_port >> 8) as u8; packet[37] = (dest_port & 0xFF) as u8;
    packet[38] = (udp_len >> 8) as u8; packet[39] = (udp_len & 0xFF) as u8;
    packet[40] = 0; packet[41] = 0; // Checksum (0 = optional/disabled in IPv4)
    
    // 4. Payload
    for i in 0..payload.len() {
        packet[42+i] = payload[i];
    }
    
    crate::net::rtl8139::send_packet(&packet);
    crate::serial_println!("UDP: Sent packet to {}.{}.{}.{}:{}", dest_ip[0], dest_ip[1], dest_ip[2], dest_ip[3], dest_port);
}
