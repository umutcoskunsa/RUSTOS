#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct EthernetHeader {
    pub dest_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: u16,
}

pub fn handle_packet(data: &[u8]) {
    if data.len() < 14 {
        return;
    }
    
    let header_ptr = data.as_ptr() as *const EthernetHeader;
    let header = unsafe { &*header_ptr };
    
    let ethertype = u16::from_be(header.ethertype);
    
    crate::serial_println!("ETHERNET: Src: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} -> Dst: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} | Type: {:#06x}",
        header.src_mac[0], header.src_mac[1], header.src_mac[2], header.src_mac[3], header.src_mac[4], header.src_mac[5],
        header.dest_mac[0], header.dest_mac[1], header.dest_mac[2], header.dest_mac[3], header.dest_mac[4], header.dest_mac[5],
        ethertype
    );
    
    match ethertype {
        0x0806 => {
            crate::serial_println!("ETHERNET: Received ARP Packet!");
            crate::net::arp::handle_packet(&data[14..]);
        },
        0x0800 => {
            crate::serial_println!("ETHERNET: Received IPv4 Packet!");
            // crate::net::ipv4::handle_packet(&data[14..]);
        },
        _ => {
            crate::serial_println!("ETHERNET: Unknown Protocol.");
        }
    }
}
