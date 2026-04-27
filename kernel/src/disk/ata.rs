/// ATA PIO (Programmed I/O) driver for LBA28 mode.
/// Communicates with the primary ATA bus via I/O ports 0x1F0–0x1F7.
use x86_64::instructions::port::Port;
use spin::Mutex;
use lazy_static::lazy_static;

// Primary ATA Bus I/O ports
const ATA_DATA:        u16 = 0x1F0;
const ATA_ERROR:       u16 = 0x1F1;
const ATA_SECTOR_CNT:  u16 = 0x1F2;
const ATA_LBA_LO:      u16 = 0x1F3;
const ATA_LBA_MID:     u16 = 0x1F4;
const ATA_LBA_HI:      u16 = 0x1F5;
const ATA_DRIVE_HEAD:  u16 = 0x1F6;
const ATA_STATUS:      u16 = 0x1F7;
const ATA_COMMAND:     u16 = 0x1F7;

// ATA Status bits
const ATA_SR_BSY:  u8 = 0x80; // Drive is busy
const ATA_SR_DRDY: u8 = 0x40; // Drive ready
const ATA_SR_ERR:  u8 = 0x01; // Error occurred

// ATA Commands
const ATA_CMD_READ_PIO:  u8 = 0x20;
const ATA_CMD_WRITE_PIO: u8 = 0x30;

pub struct AtaDrive;

pub struct AtaDriver {
    data:        Port<u16>,
    sector_cnt:  Port<u8>,
    lba_lo:      Port<u8>,
    lba_mid:     Port<u8>,
    lba_hi:      Port<u8>,
    drive_head:  Port<u8>,
    status:      Port<u8>,
    command:     Port<u8>,
}

impl AtaDriver {
    pub fn new() -> Self {
        unsafe {
            AtaDriver {
                data:       Port::new(ATA_DATA),
                sector_cnt: Port::new(ATA_SECTOR_CNT),
                lba_lo:     Port::new(ATA_LBA_LO),
                lba_mid:    Port::new(ATA_LBA_MID),
                lba_hi:     Port::new(ATA_LBA_HI),
                drive_head: Port::new(ATA_DRIVE_HEAD),
                status:     Port::new(ATA_STATUS),
                command:    Port::new(ATA_COMMAND),
            }
        }
    }

    /// Wait until the drive is no longer busy
    fn wait_ready(&mut self) {
        loop {
            let status: u8 = unsafe { self.status.read() };
            if status & ATA_SR_BSY == 0 {
                break;
            }
        }
    }

    /// Wait until the drive is ready to transfer data
    fn wait_drq(&mut self) -> Result<(), &'static str> {
        loop {
            let status: u8 = unsafe { self.status.read() };
            if status & ATA_SR_ERR != 0 {
                return Err("ATA Error during DRQ wait");
            }
            if status & ATA_SR_DRDY != 0 {
                return Ok(());
            }
        }
    }

    /// Read `count` sectors starting at `lba` into `buffer`.
    /// Buffer must have at least `count * 512` bytes.
    pub fn read_sectors(&mut self, lba: u32, count: u8, buffer: &mut [u8]) -> Result<(), &'static str> {
        assert!(buffer.len() >= count as usize * 512, "Buffer too small");
        
        self.wait_ready();

        unsafe {
            // Select master drive (0xE0) with LBA addressing + top 4 bits of LBA
            self.drive_head.write(0xE0 | ((lba >> 24) as u8 & 0x0F));
            self.sector_cnt.write(count);
            self.lba_lo.write(lba as u8);
            self.lba_mid.write((lba >> 8) as u8);
            self.lba_hi.write((lba >> 16) as u8);
            self.command.write(ATA_CMD_READ_PIO);
        }

        for i in 0..count as usize {
            self.wait_ready();
            self.wait_drq()?;

            let sector_offset = i * 512;
            // Read 256 u16 words = 512 bytes per sector
            for j in 0..256 {
                let word: u16 = unsafe { self.data.read() };
                let byte_offset = sector_offset + j * 2;
                buffer[byte_offset]     = (word & 0xFF) as u8;
                buffer[byte_offset + 1] = (word >> 8) as u8;
            }
        }

        Ok(())
    }

    /// Write `count` sectors starting at `lba` from `buffer`.
    pub fn write_sectors(&mut self, lba: u32, count: u8, buffer: &[u8]) -> Result<(), &'static str> {
        assert!(buffer.len() >= count as usize * 512, "Buffer too small");

        self.wait_ready();

        unsafe {
            self.drive_head.write(0xE0 | ((lba >> 24) as u8 & 0x0F));
            self.sector_cnt.write(count);
            self.lba_lo.write(lba as u8);
            self.lba_mid.write((lba >> 8) as u8);
            self.lba_hi.write((lba >> 16) as u8);
            self.command.write(ATA_CMD_WRITE_PIO);
        }

        for i in 0..count as usize {
            self.wait_ready();
            self.wait_drq()?;

            let sector_offset = i * 512;
            for j in 0..256 {
                let byte_offset = sector_offset + j * 2;
                let word = (buffer[byte_offset] as u16) | ((buffer[byte_offset + 1] as u16) << 8);
                unsafe { self.data.write(word); }
            }
        }

        Ok(())
    }
}

lazy_static! {
    pub static ref ATA: Mutex<AtaDriver> = Mutex::new(AtaDriver::new());
}
