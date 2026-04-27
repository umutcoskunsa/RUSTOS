pub mod ata;

use ata::ATA;

/// Read `count` sectors from LBA `lba` into `buf` using the global ATA driver.
pub fn read_sectors(lba: u32, count: u8, buf: &mut [u8]) -> Result<(), &'static str> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        ATA.lock().read_sectors(lba, count, buf)
    })
}

/// Write `count` sectors to LBA `lba` from `buf`.
pub fn write_sectors(lba: u32, count: u8, buf: &[u8]) -> Result<(), &'static str> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        ATA.lock().write_sectors(lba, count, buf)
    })
}

/// Detect whether the primary ATA master disk is present.
pub fn detect() -> bool {
    use x86_64::instructions::port::Port;
    let mut status_port: Port<u8> = unsafe { Port::new(0x1F7) };
    let status: u8 = unsafe { status_port.read() };
    // 0xFF means no device (floating bus)
    status != 0xFF
}
