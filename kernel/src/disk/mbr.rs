#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct PartitionEntry {
    pub boot_flag: u8,
    pub chs_start: [u8; 3],
    pub sys_id:    u8,
    pub chs_end:   [u8; 3],
    pub lba_start: u32,
    pub num_sectors: u32,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct MBR {
    pub bootstrap: [u8; 446],
    pub partitions: [PartitionEntry; 4],
    pub signature: u16, // 0xAA55
}

impl MBR {
    pub fn read(drive: u8) -> Option<Self> {
        let mut buf = [0u8; 512];
        if crate::disk::read_sectors(drive, 0, 1, &mut buf).is_err() {
            return None;
        }
        
        let mbr = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const MBR) };
        if mbr.signature != 0xAA55 {
            return None;
        }
        
        Some(mbr)
    }
}
