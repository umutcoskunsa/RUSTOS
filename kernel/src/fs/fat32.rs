/// Minimal FAT32 filesystem reader - no external crates required!
/// Supports: listing the root directory, reading files by name.
use alloc::string::String;
use alloc::vec::Vec;

// --- FAT32 BIOS Parameter Block (first 512 bytes of the partition) ---
#[repr(C, packed)]
struct Bpb {
    _jump:          [u8; 3],
    _oem:           [u8; 8],
    bytes_per_sec:  u16,
    sec_per_clus:   u8,
    rsvd_sec_cnt:   u16,
    num_fats:       u8,
    _root_ent_cnt:  u16,
    _total_sec16:   u16,
    _media:         u8,
    _fat_sz16:      u16,
    _sec_per_trk:   u16,
    _num_heads:     u16,
    _hidd_sec:      u32,
    _total_sec32:   u32,
    // FAT32 extended BPB
    fat_sz32:    u32,
    _ext_flags:  u16,
    _fs_ver:     u16,
    root_clus:   u32,
    _fs_info:    u16,
    _bk_boot_sec:u16,
    _reserved:   [u8; 12],
}

#[repr(C, packed)]
struct DirEntry {
    name:       [u8; 8],
    ext:        [u8; 3],
    attr:       u8,
    _reserved:  u8,
    _crt_ms:    u8,
    _crt_time:  u16,
    _crt_date:  u16,
    _lst_date:  u16,
    clus_hi:    u16,
    _wrt_time:  u16,
    _wrt_date:  u16,
    clus_lo:    u16,
    file_size:  u32,
}

const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_LFN:       u8 = 0x0F; // Long File Name entry

struct Fat32 {
    bytes_per_sec: u32,
    sec_per_clus:  u32,
    fat_start_sec: u32,
    fat_sz32:      u32,
    data_start_sec:u32,
    root_cluster:  u32,
}

impl Fat32 {
    /// Parse the BPB from sector 0 and return a Fat32 context.
    fn from_disk() -> Option<Self> {
        let mut buf = [0u8; 512];
        crate::disk::read_sectors(0, 0, 1, &mut buf).ok()?;

        // Safety: the buffer is exactly the right size and properly aligned
        let bpb = unsafe { &*(buf.as_ptr() as *const Bpb) };

        let bytes_per_sec = bpb.bytes_per_sec as u32;
        let sec_per_clus  = bpb.sec_per_clus as u32;
        let rsvd          = bpb.rsvd_sec_cnt as u32;
        let num_fats      = bpb.num_fats as u32;
        let fat_sz32    = bpb.fat_sz32;
        let root_cluster  = bpb.root_clus;

        let fat_start_sec  = rsvd;
        let data_start_sec = rsvd + num_fats * fat_sz32;

        if bytes_per_sec == 0 { return None; }

        Some(Fat32 { bytes_per_sec, sec_per_clus, fat_start_sec, fat_sz32, data_start_sec, root_cluster })
    }

    /// Convert a cluster number to the first sector of that cluster.
    fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.data_start_sec + (cluster - 2) * self.sec_per_clus
    }

    /// Read the next cluster from the FAT chain. Returns 0x0FFFFFF8+ if end of chain.
    fn next_cluster(&self, cluster: u32) -> u32 {
        let fat_offset  = cluster * 4;
        let fat_sector  = self.fat_start_sec + fat_offset / self.bytes_per_sec;
        let entry_offset = (fat_offset % self.bytes_per_sec) as usize;

        let mut buf = [0u8; 512];
        if crate::disk::read_sectors(0, fat_sector, 1, &mut buf).is_err() {
            return 0x0FFF_FFFF;
        }
        let next = u32::from_le_bytes([
            buf[entry_offset],
            buf[entry_offset + 1],
            buf[entry_offset + 2],
            buf[entry_offset + 3],
        ]) & 0x0FFF_FFFF;
        next
    }

    /// Read all sectors of a cluster chain starting at `start_cluster`.
    fn read_chain(&self, start_cluster: u32) -> Vec<u8> {
        if start_cluster == 0 {
            return Vec::new();
        }
        let mut data = Vec::new();
        let mut cluster = start_cluster;
        let secs = self.sec_per_clus as u8;
        let bytes = self.sec_per_clus as usize * self.bytes_per_sec as usize;

        while cluster < 0x0FFF_FFF8 {
            let sector = self.cluster_to_sector(cluster);
            let prev_len = data.len();
            data.resize(prev_len + bytes, 0u8);
            if crate::disk::read_sectors(0, sector, secs, &mut data[prev_len..]).is_err() {
                break;
            }
            cluster = self.next_cluster(cluster);
        }
        data
    }

    /// Parse 8.3 filename: "HELLO   TXT" → "HELLO.TXT"
    fn parse_83_name(entry: &DirEntry) -> String {
        let name = core::str::from_utf8(&entry.name).unwrap_or("").trim_end();
        let ext  = core::str::from_utf8(&entry.ext).unwrap_or("").trim_end();
        if ext.is_empty() {
            String::from(name)
        } else {
            alloc::format!("{}.{}", name, ext)
        }
    }

    /// Collect all non-hidden entries from the root directory.
    fn list_root(&self) -> Vec<String> {
        let data = self.read_chain(self.root_cluster);
        let entry_size = core::mem::size_of::<DirEntry>();
        let mut names = Vec::new();

        let mut i = 0;
        while i + entry_size <= data.len() {
            let entry = unsafe { &*(data[i..].as_ptr() as *const DirEntry) };
            if entry.name[0] == 0x00 { break; }          // end of directory
            if entry.name[0] == 0xE5 { i += entry_size; continue; } // deleted
            if entry.attr == ATTR_LFN || entry.attr == ATTR_VOLUME_ID {
                i += entry_size; continue;
            }
            let name = Self::parse_83_name(entry);
            if !name.starts_with('.') {
                names.push(name);
            }
            i += entry_size;
        }
        names
    }

    /// Find and read a file by its uppercase 8.3 name from the root directory.
    fn read_file(&self, filename: &str) -> Option<Vec<u8>> {
        let data = self.read_chain(self.root_cluster);
        let entry_size = core::mem::size_of::<DirEntry>();

        let mut i = 0;
        while i + entry_size <= data.len() {
            let entry = unsafe { &*(data[i..].as_ptr() as *const DirEntry) };
            if entry.name[0] == 0x00 { break; }
            if entry.name[0] == 0xE5 { i += entry_size; continue; }
            if entry.attr == ATTR_LFN || entry.attr & ATTR_DIRECTORY != 0 {
                i += entry_size; continue;
            }

            let name = Self::parse_83_name(entry);
            if name.eq_ignore_ascii_case(filename) {
                let cluster = ((entry.clus_hi as u32) << 16) | (entry.clus_lo as u32);
                let size    = entry.file_size as usize;
                let mut file_data = self.read_chain(cluster);
                file_data.truncate(size);
                return Some(file_data);
            }
            i += entry_size;
        }
        None
    }

    // ---- WRITE SUPPORT ----

    /// Write a 32-bit FAT entry for `cluster`, preserving the top 4 bits.
    fn write_fat_entry(&self, cluster: u32, value: u32) -> Option<()> {
        let fat_offset   = cluster * 4;
        let fat_sector   = self.fat_start_sec + fat_offset / self.bytes_per_sec;
        let entry_offset = (fat_offset % self.bytes_per_sec) as usize;

        let mut buf = [0u8; 512];
        crate::disk::read_sectors(0, fat_sector, 1, &mut buf).ok()?;

        let v = (value & 0x0FFF_FFFF) | (u32::from_le_bytes([
            buf[entry_offset], buf[entry_offset+1], buf[entry_offset+2], buf[entry_offset+3]
        ]) & 0xF000_0000);
        let bytes = v.to_le_bytes();
        buf[entry_offset..entry_offset+4].copy_from_slice(&bytes);
        crate::disk::write_sectors(0, fat_sector, 1, &buf).ok()?;
        Some(())
    }

    /// Scan the FAT for the first free cluster (value == 0). Returns None if disk is full.
    fn find_free_cluster(&self) -> Option<u32> {
        let entries_per_sec = self.bytes_per_sec / 4;
        let mut buf = [0u8; 512];
        for sec_off in 0..self.fat_sz32 {
            let sector = self.fat_start_sec + sec_off;
            if crate::disk::read_sectors(0, sector, 1, &mut buf).is_err() { break; }
            for i in 0..entries_per_sec as usize {
                let entry = u32::from_le_bytes([buf[i*4], buf[i*4+1], buf[i*4+2], buf[i*4+3]]) & 0x0FFF_FFFF;
                let cluster = sec_off * entries_per_sec + i as u32;
                if cluster >= 2 && entry == 0 {
                    return Some(cluster);
                }
            }
        }
        None
    }

    /// Free an entire cluster chain in the FAT (mark every cluster as 0).
    fn free_chain(&self, mut cluster: u32) {
        while cluster >= 2 && cluster < 0x0FFF_FFF8 {
            let next = self.next_cluster(cluster);
            let _ = self.write_fat_entry(cluster, 0);
            cluster = next;
        }
    }

    /// Allocate a fresh cluster chain of exactly `cluster_count` clusters.
    /// Returns the first cluster of the chain, or None if out of space.
    fn alloc_chain(&self, cluster_count: usize) -> Option<u32> {
        let mut prev: Option<u32> = None;
        let mut head: Option<u32> = None;
        for _ in 0..cluster_count {
            let c = self.find_free_cluster()?;
            // Mark as end-of-chain while we link it up
            self.write_fat_entry(c, 0x0FFF_FFFF)?;
            if let Some(p) = prev {
                self.write_fat_entry(p, c)?;
            } else {
                head = Some(c);
            }
            prev = Some(c);
        }
        head
    }

    /// Write `data` into an existing cluster chain starting at `start_cluster`.
    fn write_chain(&self, start_cluster: u32, data: &[u8]) -> Option<()> {
        let cluster_bytes = self.sec_per_clus as usize * self.bytes_per_sec as usize;
        let mut cluster = start_cluster;
        let mut offset  = 0;

        while cluster < 0x0FFF_FFF8 && offset < data.len() {
            let sector = self.cluster_to_sector(cluster);
            let chunk_len = cluster_bytes.min(data.len() - offset);

            // Pad the last cluster with zeros if needed
            let mut sector_buf = alloc::vec![0u8; cluster_bytes];
            sector_buf[..chunk_len].copy_from_slice(&data[offset..offset + chunk_len]);

            crate::disk::write_sectors(0, sector, self.sec_per_clus as u8, &sector_buf).ok()?;
            offset  += chunk_len;
            cluster  = self.next_cluster(cluster);
        }
        Some(())
    }

    /// Convert a filename like "hello.txt" to an 8.3 name/ext pair (uppercase, space-padded).
    fn format_83(filename: &str) -> Option<([u8; 8], [u8; 3])> {
        let mut name = [b' '; 8];
        let mut ext  = [b' '; 3];

        let dot = filename.rfind('.');
        let base = if let Some(d) = dot { &filename[..d] } else { filename };
        let extension = if let Some(d) = dot { &filename[d+1..] } else { "" };

        if base.is_empty() || base.len() > 8 || extension.len() > 3 { return None; }

        for (i, b) in base.bytes().enumerate() {
            name[i] = b.to_ascii_uppercase();
        }
        for (i, b) in extension.bytes().enumerate() {
            ext[i] = b.to_ascii_uppercase();
        }
        Some((name, ext))
    }

    /// Write (create or overwrite) a file in the root directory.
    fn write_file_inner(&self, filename: &str, data: &[u8]) -> Option<()> {
        let (name83, ext83) = Self::format_83(filename)?;
        let cluster_bytes = self.sec_per_clus as usize * self.bytes_per_sec as usize;
        let clusters_needed = (data.len() + cluster_bytes - 1) / cluster_bytes;

        // Walk root directory cluster chain to find the sector that holds this entry
        let entry_size = core::mem::size_of::<DirEntry>();
        let mut entry_sector_abs: Option<u32>  = None;
        let mut entry_byte_off:   Option<usize> = None;
        let mut existing_cluster: Option<u32>   = None;

        let mut cluster = self.root_cluster;
        'outer: while cluster < 0x0FFF_FFF8 {
            let first_sector = self.cluster_to_sector(cluster);
            for sec_off in 0..self.sec_per_clus {
                let sector = first_sector + sec_off;
                let mut sbuf = [0u8; 512];
                if crate::disk::read_sectors(0, sector, 1, &mut sbuf).is_err() { break; }

                for e in 0..(512 / entry_size) {
                    let off = e * entry_size;
                    let entry = unsafe { &*(sbuf[off..].as_ptr() as *const DirEntry) };

                    if entry.name[0] == 0x00 {
                        // Free slot: use for new file if we haven't found the file yet
                        if entry_sector_abs.is_none() {
                            entry_sector_abs = Some(sector);
                            entry_byte_off   = Some(off);
                        }
                        break 'outer;
                    }
                    if entry.name[0] == 0xE5 {
                        // Deleted slot: candidate for new file
                        if entry_sector_abs.is_none() {
                            entry_sector_abs = Some(sector);
                            entry_byte_off   = Some(off);
                        }
                        continue;
                    }
                    if entry.attr == ATTR_LFN || entry.attr == ATTR_VOLUME_ID { continue; }

                    // Check for matching filename
                    if entry.name == name83 && entry.ext == ext83 {
                        entry_sector_abs = Some(sector);
                        entry_byte_off   = Some(off);
                        existing_cluster = Some(((entry.clus_hi as u32) << 16) | (entry.clus_lo as u32));
                        break 'outer;
                    }
                }
            }
            cluster = self.next_cluster(cluster);
        }

        // Free old cluster chain so we can allocate a fresh one
        if let Some(ec) = existing_cluster {
            if ec >= 2 { self.free_chain(ec); }
        }

        // Allocate a new chain for the data (even for empty files, allocate 1 cluster)
        let n = if clusters_needed == 0 { 1 } else { clusters_needed };
        let start_cluster = self.alloc_chain(n)?;

        // Write the data
        self.write_chain(start_cluster, data)?;

        // Write the directory entry
        let abs_sector = entry_sector_abs?;
        let byte_off   = entry_byte_off?;
        let mut sbuf   = [0u8; 512];
        crate::disk::read_sectors(0, abs_sector, 1, &mut sbuf).ok()?;

        let entry_mut = unsafe { &mut *(sbuf[byte_off..].as_mut_ptr() as *mut DirEntry) };
        entry_mut.name       = name83;
        entry_mut.ext        = ext83;
        entry_mut.attr       = 0x20; // Archive
        entry_mut._reserved  = 0;
        entry_mut._crt_ms    = 0;
        entry_mut._crt_time  = 0;
        entry_mut._crt_date  = 0x5421; // Fake date: 2022-01-01
        entry_mut._lst_date  = 0x5421;
        entry_mut.clus_hi    = ((start_cluster >> 16) & 0xFFFF) as u16;
        entry_mut._wrt_time  = 0;
        entry_mut._wrt_date  = 0x5421;
        entry_mut.clus_lo    = (start_cluster & 0xFFFF) as u16;
        entry_mut.file_size  = data.len() as u32;

        crate::disk::write_sectors(0, abs_sector, 1, &sbuf).ok()?;
        Some(())
    }

    /// Delete a file from the root directory.
    fn delete_file(&self, filename: &str) -> Option<()> {
        let (name83, ext83) = Self::format_83(filename)?;
        let data = self.read_chain(self.root_cluster);
        let entry_size = core::mem::size_of::<DirEntry>();

        let mut i = 0;
        while i + entry_size <= data.len() {
            let entry = unsafe { &*(data[i..].as_ptr() as *const DirEntry) };
            if entry.name[0] == 0x00 { break; }
            if entry.name[0] == 0xE5 { i += entry_size; continue; }
            if entry.attr == ATTR_LFN { i += entry_size; continue; }

            if entry.name == name83 && entry.ext == ext83 {
                let cluster = ((entry.clus_hi as u32) << 16) | (entry.clus_lo as u32);
                if cluster >= 2 { self.free_chain(cluster); }

                // Calculate which sector and offset this entry is in
                let cluster_bytes = self.sec_per_clus as usize * self.bytes_per_sec as usize;
                let root_offset = i;
                
                // We need to re-find which root cluster/sector this is
                let mut current_root_clus = self.root_cluster;
                let mut bytes_skipped = 0;
                while current_root_clus < 0x0FFF_FFF8 {
                    if root_offset < bytes_skipped + cluster_bytes {
                        let off_in_cluster = root_offset - bytes_skipped;
                        let sector = self.cluster_to_sector(current_root_clus) + (off_in_cluster as u32 / self.bytes_per_sec);
                        let off_in_sector = off_in_cluster % self.bytes_per_sec as usize;

                        let mut sbuf = [0u8; 512];
                        crate::disk::read_sectors(0, sector, 1, &mut sbuf).ok()?;
                        sbuf[off_in_sector] = 0xE5; // Mark as deleted
                        crate::disk::write_sectors(0, sector, 1, &sbuf).ok()?;
                        return Some(());
                    }
                    bytes_skipped += cluster_bytes;
                    current_root_clus = self.next_cluster(current_root_clus);
                }
            }
            i += entry_size;
        }
        None
    }

    /// Rename a file in the root directory.
    fn rename_file(&self, old_filename: &str, new_filename: &str) -> Option<()> {
        let (old_name83, old_ext83) = Self::format_83(old_filename)?;
        let (new_name83, new_ext83) = Self::format_83(new_filename)?;
        
        let data = self.read_chain(self.root_cluster);
        let entry_size = core::mem::size_of::<DirEntry>();

        let mut i = 0;
        while i + entry_size <= data.len() {
            let entry = unsafe { &*(data[i..].as_ptr() as *const DirEntry) };
            if entry.name[0] == 0x00 { break; }
            if entry.name[0] == 0xE5 { i += entry_size; continue; }
            if entry.attr == ATTR_LFN { i += entry_size; continue; }

            if entry.name == old_name83 && entry.ext == old_ext83 {
                // Found it! Now update the name in the directory
                let cluster_bytes = self.sec_per_clus as usize * self.bytes_per_sec as usize;
                let root_offset = i;
                
                let mut current_root_clus = self.root_cluster;
                let mut bytes_skipped = 0;
                while current_root_clus < 0x0FFF_FFF8 {
                    if root_offset < bytes_skipped + cluster_bytes {
                        let off_in_cluster = root_offset - bytes_skipped;
                        let sector = self.cluster_to_sector(current_root_clus) + (off_in_cluster as u32 / self.bytes_per_sec);
                        let off_in_sector = off_in_cluster % self.bytes_per_sec as usize;

                        let mut sbuf = [0u8; 512];
                        crate::disk::read_sectors(0, sector, 1, &mut sbuf).ok()?;
                        
                        let entry_mut = unsafe { &mut *(sbuf[off_in_sector..].as_mut_ptr() as *mut DirEntry) };
                        entry_mut.name = new_name83;
                        entry_mut.ext  = new_ext83;
                        
                        crate::disk::write_sectors(0, sector, 1, &sbuf).ok()?;
                        return Some(());
                    }
                    bytes_skipped += cluster_bytes;
                    current_root_clus = self.next_cluster(current_root_clus);
                }
            }
            i += entry_size;
        }
        None
    }
}

// --- VFS Implementation ---

pub struct Fat32Fs;

impl Fat32Fs {
    pub fn new() -> Self {
        Fat32Fs
    }
}

impl super::FileSystem for Fat32Fs {
    fn read_file(&self, filename: &str, _uid: u16, _gid: u16) -> Option<Vec<u8>> {
        Fat32::from_disk()?.read_file(filename)
    }

    fn write_file(&self, filename: &str, data: &[u8], _uid: u16, _gid: u16) -> bool {
        Fat32::from_disk()
            .and_then(|fs| fs.write_file_inner(filename, data))
            .is_some()
    }

    fn delete_file(&self, filename: &str, _uid: u16, _gid: u16) -> bool {
        Fat32::from_disk()
            .and_then(|fs| fs.delete_file(filename))
            .is_some()
    }

    fn rename_file(&self, old_name: &str, new_name: &str, _uid: u16, _gid: u16) -> bool {
        Fat32::from_disk()
            .and_then(|fs| fs.rename_file(old_name, new_name))
            .is_some()
    }

    fn list_dir(&self, _path: &str, _uid: u16, _gid: u16) -> Vec<String> {
        Fat32::from_disk().map(|fs| fs.list_root()).unwrap_or_default()
    }

    fn create_dir(&self, _path: &str, _uid: u16, _gid: u16) -> bool {
        false
    }
    fn create_symlink(&self, _path: &str, _target: &str, _uid: u16, _gid: u16) -> bool {
        false
    }
    fn chmod(&self, _path: &str, _mode: u16, _uid: u16, _gid: u16) -> bool {
        false
    }
    fn chown(&self, _path: &str, _new_uid: u16, _new_gid: u16, _uid: u16, _gid: u16) -> bool {
        false
    }
}
