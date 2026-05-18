pub mod rtl8139;
pub mod ethernet;
pub mod arp;
pub mod ipv4;
pub mod udp;

pub fn init() {
    crate::serial_println!("NET: Initializing Network Stack...");
    rtl8139::init();
}
