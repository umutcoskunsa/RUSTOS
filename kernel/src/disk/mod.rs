pub mod ata;
pub mod mbr;

use ata::ATA;

/// Read `count` sectors from LBA `lba` from a specific drive into `buf`.
/// drive: 0 for Master, 1 for Slave
pub fn read_sectors(drive: u8, lba: u32, count: u8, buf: &mut [u8]) -> Result<(), &'static str> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        ATA.lock().read_sectors(drive, lba, count, buf)
    })
}

/// Write `count` sectors to LBA `lba` on a specific drive from `buf`.
/// drive: 0 for Master, 1 for Slave
pub fn write_sectors(drive: u8, lba: u32, count: u8, buf: &[u8]) -> Result<(), &'static str> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        ATA.lock().write_sectors(drive, lba, count, buf)
    })
}

/// Detect whether a specific ATA drive is present.
/// drive: 0 for Master, 1 for Slave
pub fn detect(drive: u8) -> bool {
    use x86_64::instructions::port::Port;
    let mut drive_head_port: Port<u8> = unsafe { Port::new(0x1F6) };
    let mut status_port: Port<u8> = unsafe { Port::new(0x1F7) };
    
    unsafe {
        // Select the drive
        let drive_select = if drive == 0 { 0xE0 } else { 0xF0 };
        drive_head_port.write(drive_select);
        
        // Small delay
        for _ in 0..1000 { x86_64::instructions::nop(); }
        
        let status: u8 = status_port.read();
        // 0xFF means no device (floating bus)
        status != 0xFF
    }
}
