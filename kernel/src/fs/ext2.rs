use alloc::vec::Vec;
use alloc::string::String;
use crate::fs::FileSystem;
use spin::Mutex;

/// ext2 Superblock (located at 1024 bytes from the start of the partition)
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct Superblock {
    pub s_inodes_count:      u32,
    pub s_blocks_count:      u32,
    pub s_r_blocks_count:    u32,
    pub s_free_blocks_count: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block:  u32,
    pub s_log_block_size:    u32,
    pub s_log_frag_size:     i32,
    pub s_blocks_per_group:  u32,
    pub s_frags_per_group:   u32,
    pub s_inodes_per_group:  u32,
    pub s_mtime:             u32,
    pub s_wtime:             u32,
    pub s_mnt_count:         u16,
    pub s_max_mnt_count:     u16,
    pub s_magic:             u16, // 0xEF53
    pub s_state:             u16,
    pub s_errors:            u16,
    pub s_minor_rev_level:   u16,
    pub s_lastcheck:         u32,
    pub s_checkinterval:     u32,
    pub s_creator_os:        u32,
    pub s_rev_level:         u32,
    pub s_def_resuid:        u16,
    pub s_def_resgid:        u16,
    // EXT2_DYNAMIC_REV specific fields
    pub s_first_ino:         u32,
    pub s_inode_size:        u16,
    pub s_block_group_nr:    u16,
    pub s_feature_compat:    u32,
    pub s_feature_incompat:  u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid:              [u8; 16],
    pub s_volume_name:       [u8; 16],
    pub s_last_mounted:      [u8; 64],
    pub s_algo_bitmap:       u32,
    // ... padding to 1024 bytes
    pub _reserved:           [u8; 812],
}

/// ext2 Block Group Descriptor
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct BlockGroupDescriptor {
    pub bg_block_bitmap:      u32,
    pub bg_inode_bitmap:      u32,
    pub bg_inode_table:       u32,
    pub bg_free_blocks_count: u16,
    pub bg_free_inodes_count: u16,
    pub bg_used_dirs_count:   u16,
    pub bg_pad:               u16,
    pub _reserved:            [u8; 12],
}

/// ext2 Inode structure
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct Inode {
    pub i_mode:        u16,
    pub i_uid:         u16,
    pub i_size:        u32,
    pub i_atime:       u32,
    pub i_ctime:       u32,
    pub i_mtime:       u32,
    pub i_dtime:       u32,
    pub i_gid:         u16,
    pub i_links_count: u16,
    pub i_blocks:      u32,
    pub i_flags:       u32,
    pub i_osd1:        u32,
    pub i_block:       [u32; 15],
    pub i_generation:  u32,
    pub i_file_acl:    u32,
    pub i_dir_acl:     u32,
    pub i_faddr:       u32,
    pub i_osd2:        [u8; 12],
}

/// ext2 directory entry structure
#[repr(C, packed)]
#[derive(Debug, Copy, Clone)]
pub struct DirEntry {
    pub inode:      u32,
    pub rec_len:    u16,
    pub name_len:   u8,
    pub file_type:  u8,
}

pub use alloc::collections::BTreeMap;

/// A simple block cache to avoid redundant disk I/O.
struct BlockCache {
    entries: BTreeMap<u32, Vec<u8>>,
    max_size: usize,
}

impl BlockCache {
    fn new(max_size: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            max_size,
        }
    }

    fn get(&self, block_id: u32) -> Option<Vec<u8>> {
        self.entries.get(&block_id).cloned()
    }

    fn insert(&mut self, block_id: u32, data: Vec<u8>) {
        if self.entries.len() >= self.max_size {
            // Simple replacement: just clear everything if full
            // (A real LRU would be better but this is simple and safe)
            self.entries.clear();
        }
        self.entries.insert(block_id, data);
    }

    fn remove(&mut self, block_id: u32) {
        self.entries.remove(&block_id);
    }
}

struct Ext2Inner {
    drive_id:          u8,
    start_lba:         u32, // Added for partition support
    superblock:        Superblock,
    group_descriptors: Vec<BlockGroupDescriptor>,
    block_size:        u32,
    block_cache:       BlockCache,
    inode_cache:       BTreeMap<u32, Inode>,
}

pub struct Ext2Fs {
    pub inner: Mutex<Ext2Inner>,
}

impl Ext2Fs {
    /// Read a single block from the disk into a Vec, checking the cache first.
    fn read_block(&self, block_id: u32) -> Option<Vec<u8>> {
        if block_id == 0 { return None; }
        
        // 1. Try Cache
        if let Some(cached) = self.inner.lock().block_cache.get(block_id) {
            return Some(cached);
        }

        // 2. Read from disk
        let (drive_id, start_lba, block_size) = {
            let inner = self.inner.lock();
            (inner.drive_id, inner.start_lba, inner.block_size)
        };
        
        let data = self.read_block_internal(block_id, drive_id, start_lba, block_size)?;
        
        // 3. Insert into cache
        self.inner.lock().block_cache.insert(block_id, data.clone());
        Some(data)
    }

    fn read_block_internal(&self, block_id: u32, drive_id: u8, start_lba: u32, block_size: u32) -> Option<Vec<u8>> {
        let mut data = alloc::vec![0u8; block_size as usize];
        let sectors_per_block = block_size / 512;
        let lba = start_lba + (block_id * sectors_per_block);
        if crate::disk::read_sectors(drive_id, lba, sectors_per_block as u8, &mut data).is_err() {
            return None;
        }
        Some(data)
    }

    /// Write a single block to the disk and update the cache.
    fn write_block(&self, block_id: u32, data: &[u8]) -> bool {
        if block_id == 0 { return false; }
        let (drive_id, start_lba, block_size) = {
            let inner = self.inner.lock();
            (inner.drive_id, inner.start_lba, inner.block_size)
        };
        
        if !self.write_block_internal(block_id, data, drive_id, start_lba, block_size) {
            return false;
        }

        // 2. Update cache
        self.inner.lock().block_cache.insert(block_id, data.to_vec());
        true
    }

    fn write_block_internal(&self, block_id: u32, data: &[u8], drive_id: u8, start_lba: u32, block_size: u32) -> bool {
        let sectors_per_block = block_size / 512;
        let lba = start_lba + (block_id * sectors_per_block);
        crate::disk::write_sectors(drive_id, lba, sectors_per_block as u8, data).is_ok()
    }

    /// Find and allocate a free block. Returns block ID.
    fn allocate_block(&self) -> Option<u32> {
        let mut inner = self.inner.lock();
        for g in 0..inner.group_descriptors.len() {
            if inner.group_descriptors[g].bg_free_blocks_count > 0 {
                let bitmap_block = inner.group_descriptors[g].bg_block_bitmap;
                drop(inner);
                let mut bitmap = self.read_block(bitmap_block)?;
                inner = self.inner.lock();

                for i in 0..bitmap.len() {
                    if bitmap[i] != 0xFF {
                        for bit in 0..8 {
                            if (bitmap[i] & (1 << bit)) == 0 {
                                bitmap[i] |= 1 << bit;
                                let block_id = (g as u32 * inner.superblock.s_blocks_per_group) + (i as u32 * 8) + bit + inner.superblock.s_first_data_block;
                                
                                inner.group_descriptors[g].bg_free_blocks_count -= 1;
                                inner.superblock.s_free_blocks_count -= 1;
                                drop(inner);

                                self.write_block(bitmap_block, &bitmap);
                                return Some(block_id);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Find and allocate a free inode. Returns inode ID (1-indexed).
    fn allocate_inode(&self) -> Option<u32> {
        let mut inner = self.inner.lock();
        for g in 0..inner.group_descriptors.len() {
            if inner.group_descriptors[g].bg_free_inodes_count > 0 {
                let bitmap_block = inner.group_descriptors[g].bg_inode_bitmap;
                drop(inner);
                let mut bitmap = self.read_block(bitmap_block)?;
                inner = self.inner.lock();

                for i in 0..bitmap.len() {
                    if bitmap[i] != 0xFF {
                        for bit in 0..8 {
                            if (bitmap[i] & (1 << bit)) == 0 {
                                bitmap[i] |= 1 << bit;
                                let inode_id = (g as u32 * inner.superblock.s_inodes_per_group) + (i as u32 * 8) + bit + 1;
                                
                                inner.group_descriptors[g].bg_free_inodes_count -= 1;
                                inner.superblock.s_free_inodes_count -= 1;
                                drop(inner);

                                self.write_block(bitmap_block, &bitmap);
                                return Some(inode_id);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn deallocate_inode(&self, inode_id: u32) {
        if inode_id == 0 { return; }
        let mut inner = self.inner.lock();
        let inodes_per_group = inner.superblock.s_inodes_per_group;
        let g = ((inode_id - 1) / inodes_per_group) as usize;
        let index = (inode_id - 1) % inodes_per_group;
        
        if g >= inner.group_descriptors.len() { return; }
        let bitmap_block = inner.group_descriptors[g].bg_inode_bitmap;
        
        drop(inner);
        let mut bitmap = match self.read_block(bitmap_block) {
            Some(b) => b,
            None => return,
        };
        
        let byte = (index / 8) as usize;
        let bit = (index % 8) as u8;
        bitmap[byte] &= !(1 << bit);
        
        inner = self.inner.lock();
        inner.group_descriptors[g].bg_free_inodes_count += 1;
        inner.superblock.s_free_inodes_count += 1;
        drop(inner);
        
        self.write_block(bitmap_block, &bitmap);

        // Remove from Inode Cache
        self.inner.lock().inode_cache.remove(&inode_id);
    }

    fn deallocate_block(&self, block_id: u32) {
        if block_id == 0 { return; }
        let mut inner = self.inner.lock();
        let blocks_per_group = inner.superblock.s_blocks_per_group;
        let first_data = inner.superblock.s_first_data_block;
        
        let relative_id = block_id - first_data;
        let g = (relative_id / blocks_per_group) as usize;
        let index = relative_id % blocks_per_group;

        if g >= inner.group_descriptors.len() { return; }
        let bitmap_block = inner.group_descriptors[g].bg_block_bitmap;

        drop(inner);
        let mut bitmap = match self.read_block(bitmap_block) {
            Some(b) => b,
            None => return,
        };

        let byte = (index / 8) as usize;
        let bit = (index % 8) as u8;
        bitmap[byte] &= !(1 << bit);

        inner = self.inner.lock();
        inner.group_descriptors[g].bg_free_blocks_count += 1;
        inner.superblock.s_free_blocks_count += 1;
        drop(inner);

        self.write_block(bitmap_block, &bitmap);
    }

    /// Write a specific inode back to the disk.
    pub fn write_inode(&self, inode_id: u32, inode: &Inode) -> bool {
        if inode_id == 0 { return false; }
        let (block_size, drive_id, inode_table_block, byte_offset_in_block, inode_size) = {
            let mut inner = self.inner.lock();
            let block_size = inner.block_size;
            let drive_id = inner.drive_id;
            let inodes_per_group = inner.superblock.s_inodes_per_group;
            let inode_size = if inner.superblock.s_rev_level == 0 { 128 } else { inner.superblock.s_inode_size as u32 };
            
            let group = (inode_id - 1) / inodes_per_group;
            let index = (inode_id - 1) % inodes_per_group;
            
            let desc = match inner.group_descriptors.get(group as usize) {
                Some(d) => d,
                None => return false,
            };
            
            let block_offset = (index * inode_size) / block_size;
            let byte_offset_in_block = (index * inode_size) % block_size;
            let inode_table_block = desc.bg_inode_table + block_offset;
            
            // Update Inode Cache while locked
            inner.inode_cache.insert(inode_id, *inode);
            
            (block_size, drive_id, inode_table_block, byte_offset_in_block, inode_size)
        };

        // Read block (unlocked)
        let mut data = match self.read_block(inode_table_block) {
            Some(d) => d,
            None => return false,
        };

        // Update block
        let copy_size = core::cmp::min(128, inode_size as usize);
        unsafe {
            core::ptr::copy_nonoverlapping(
                inode as *const Inode as *const u8,
                data[byte_offset_in_block as usize..].as_mut_ptr(),
                copy_size
            );
        }

        // Write block back (this will update the cache and the disk)
        self.write_block(inode_table_block, &data)
    }

    /// Sync metadata to disk.
    fn sync_metadata(&self) {
        let (drive_id, start_lba, sb, bgdt_block, block_size, group_descriptors) = {
            let inner = self.inner.lock();
            let bgdt_block = if inner.block_size == 1024 { 2 } else { 1 };
            (inner.drive_id, inner.start_lba, inner.superblock, bgdt_block, inner.block_size, inner.group_descriptors.clone())
        };

        let mut sb_buf = [0u8; 1024];
        unsafe { core::ptr::write_unaligned(sb_buf.as_mut_ptr() as *mut Superblock, sb); }
        // Superblock is always at 1024 bytes (LBA 2 relative to partition start)
        crate::disk::write_sectors(drive_id, start_lba + 2, 2, &sb_buf).ok();
        
        let mut bgdt_data = alloc::vec![0u8; block_size as usize];
        let desc_size = core::mem::size_of::<BlockGroupDescriptor>();
        for (i, desc) in group_descriptors.iter().enumerate() {
            let offset = i * desc_size;
            unsafe { core::ptr::write_unaligned(bgdt_data[offset..].as_mut_ptr() as *mut BlockGroupDescriptor, *desc); }
        }
        self.write_block(bgdt_block, &bgdt_data);
    }

    /// Add a new entry to a directory.
    fn add_entry(&self, dir_inode_id: u32, name: &str, inode_id: u32, file_type: u8) -> bool {
        let dir_inode = match self.read_inode(dir_inode_id) {
            Some(i) => i,
            None => return false,
        };

        // For now, we only support single-block directories
        let block_id = dir_inode.i_block[0];
        if block_id == 0 { return false; }

        let mut data = match self.read_block(block_id) {
            Some(d) => d,
            None => return false,
        };

        let mut offset = 0;
        let name_len = name.len();
        let needed_len = (8 + name_len + 3) & !3;

        while offset < data.len() {
            let de = unsafe { &mut *(data[offset..].as_mut_ptr() as *mut DirEntry) };
            if de.rec_len < 8 { break; }

            // If entry is empty (inode == 0), see if we can use the whole rec_len
            if de.inode == 0 && de.rec_len as usize >= needed_len {
                de.inode = inode_id;
                de.name_len = name_len as u8;
                de.file_type = file_type;
                unsafe {
                    core::ptr::copy_nonoverlapping(name.as_ptr(), data[offset + 8..].as_mut_ptr(), name_len);
                }
                return self.write_block(block_id, &data);
            }

            // Otherwise, see if there is enough space to split this entry
            let actual_len = if de.inode == 0 { 0 } else { (8 + de.name_len as usize + 3) & !3 };
            let free_space = de.rec_len as usize - actual_len;

            if free_space >= needed_len {
                let old_rec_len = de.rec_len;
                de.rec_len = actual_len as u16;
                
                let next_offset = offset + actual_len;
                let new_de = unsafe { &mut *(data[next_offset..].as_mut_ptr() as *mut DirEntry) };
                new_de.inode = inode_id;
                new_de.rec_len = old_rec_len - actual_len as u16;
                new_de.name_len = name_len as u8;
                new_de.file_type = file_type;
                unsafe {
                    core::ptr::copy_nonoverlapping(name.as_ptr(), data[next_offset + 8..].as_mut_ptr(), name_len);
                }
                return self.write_block(block_id, &data);
            }

            offset += de.rec_len as usize;
        }
        false
    }

    /// Remove an entry from a directory.
    fn remove_entry(&self, dir_inode_id: u32, name: &str) -> bool {
        let dir_inode = match self.read_inode(dir_inode_id) {
            Some(i) => i,
            None => return false,
        };

        let block_id = dir_inode.i_block[0];
        if block_id == 0 { return false; }

        let mut data = match self.read_block(block_id) {
            Some(d) => d,
            None => return false,
        };

        let mut offset = 0;
        let mut prev_offset: Option<usize> = None;

        while offset < data.len() {
            let de = unsafe { &mut *(data[offset..].as_mut_ptr() as *mut DirEntry) };
            if de.rec_len < 8 { break; }

            // Safe name comparison
            let name_ptr = data[offset + 8..].as_ptr();
            let mut matches = de.name_len as usize == name.len();
            if matches {
                for i in 0..name.len() {
                    if unsafe { *name_ptr.add(i) } != name.as_bytes()[i] {
                        matches = false;
                        break;
                    }
                }
            }

            if matches && de.inode != 0 {
                if let Some(p_off) = prev_offset {
                    let prev_de = unsafe { &mut *(data[p_off..].as_mut_ptr() as *mut DirEntry) };
                    prev_de.rec_len += de.rec_len;
                } else {
                    de.inode = 0;
                }
                return self.write_block(block_id, &data);
            }

            prev_offset = Some(offset);
            offset += de.rec_len as usize;
        }
        false
    }

    pub fn try_new(drive_id: u8, start_lba: u32) -> Option<Self> {
        let mut buf = [0u8; 1024];
        // Read superblock (always at offset 1024)
        if crate::disk::read_sectors(drive_id, start_lba + 2, 2, &mut buf).is_err() { 
            crate::serial_println!("EXT2: Failed to read superblock from drive {} at LBA {}", drive_id, start_lba + 2);
            return None; 
        }
        let sb = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const Superblock) };
        let magic = sb.s_magic;
        if magic != 0xEF53 { 
            crate::serial_println!("EXT2: Invalid magic 0x{:04X} on drive {} (expected 0xEF53)", magic, drive_id);
            return None; 
        }
        let block_size = 1024 << sb.s_log_block_size;
        let group_count = (sb.s_blocks_count + sb.s_blocks_per_group - 1) / sb.s_blocks_per_group;
        let bgdt_block = if block_size == 1024 { 2 } else { 1 };
        
        let fs = Self {
            inner: Mutex::new(Ext2Inner {
                drive_id,
                start_lba,
                superblock: sb,
                block_size,
                group_descriptors: Vec::new(),
                block_cache: BlockCache::new(128),
                inode_cache: BTreeMap::new(),
            })
        };
        
        let bgdt_data = fs.read_block(bgdt_block)?;
        let mut descriptors = Vec::new();
        let desc_size = core::mem::size_of::<BlockGroupDescriptor>();
        for i in 0..group_count as usize {
            let offset = i * desc_size;
            if offset + desc_size > bgdt_data.len() { break; }
            descriptors.push(unsafe { core::ptr::read_unaligned(bgdt_data[offset..].as_ptr() as *const BlockGroupDescriptor) });
        }

        fs.inner.lock().group_descriptors = descriptors;

        let blocks_count = sb.s_blocks_count;
        crate::serial_println!("EXT2: Mounted drive {}. Blocks: {}, Groups: {}, Block Size: {}", 
            drive_id, blocks_count, group_count, block_size);

        Some(fs)
    }

    pub fn read_inode(&self, inode_id: u32) -> Option<Inode> {
        if inode_id == 0 { return None; }
        
        // 1. Try Cache
        if let Some(cached) = self.inner.lock().inode_cache.get(&inode_id) {
            return Some(*cached);
        }

        let (inodes_per_group, group_descriptors, s_rev_level, s_inode_size, block_size) = {
            let inner = self.inner.lock();
            (inner.superblock.s_inodes_per_group, inner.group_descriptors.clone(), inner.superblock.s_rev_level, inner.superblock.s_inode_size, inner.block_size)
        };
        let group = (inode_id - 1) / inodes_per_group;
        let index = (inode_id - 1) % inodes_per_group;
        let desc = group_descriptors.get(group as usize)?;
        let inode_size = if s_rev_level == 0 { 128 } else { s_inode_size as u32 };
        let block_offset = (index * inode_size) / block_size;
        let offset_in_block = (index * inode_size) % block_size;
        let data = self.read_block(desc.bg_inode_table + block_offset)?;
        let inode = unsafe { core::ptr::read_unaligned(data[offset_in_block as usize..].as_ptr() as *const Inode) };
        
        // 2. Update Cache
        self.inner.lock().inode_cache.insert(inode_id, inode);
        Some(inode)
    }

    pub fn list_directory(&self, inode: &Inode) -> Vec<(String, u32)> {
        let mut entries = Vec::new();
        if let Some(data) = self.read_block(inode.i_block[0]) {
            let mut offset = 0;
            while offset + 8 <= data.len() {
                let de = unsafe { core::ptr::read_unaligned(data[offset..].as_ptr() as *const DirEntry) };
                if de.inode != 0 {
                    let name_start = offset + 8;
                    let name_end = name_start + de.name_len as usize;
                    if name_end <= data.len() {
                        if let Ok(name) = core::str::from_utf8(&data[name_start..name_end]) {
                            entries.push((String::from(name), de.inode));
                        }
                    }
                }
                if de.rec_len == 0 { break; }
                offset += de.rec_len as usize;
            }
        }
        entries
    }

    /// Check if a given UID/GID has the requested permissions (1=exec, 2=write, 4=read)
    pub fn check_access(&self, inode: &Inode, mask: u16, uid: u16, gid: u16) -> bool {
        let mode = inode.i_mode;
        
        // Root (UID 0) always has access
        if uid == 0 { return true; }

        let mut perm = 0;
        if uid == inode.i_uid {
            // Owner bits
            perm = (mode >> 6) & 7;
        } else if gid == inode.i_gid {
            // Group bits
            perm = (mode >> 3) & 7;
        } else {
            // Other bits
            perm = mode & 7;
        }

        (perm & mask) == mask
    }

    pub fn resolve_path(&self, path: &str) -> Option<u32> {
        self.resolve_path_recursive(path, 2, 0)
    }

    fn resolve_path_recursive(&self, path: &str, start_inode: u32, depth: u8) -> Option<u32> {
        if depth > 8 { return None; } // Recursion limit

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_inode_id = if path.starts_with('/') { 2 } else { start_inode };
        
        // If path is empty (and relative), return starting point
        if parts.is_empty() && !path.starts_with('/') { return Some(current_inode_id); }
        if parts.is_empty() && path.starts_with('/') { return Some(2); }

        for (i, part) in parts.iter().enumerate() {
            let inode = self.read_inode(current_inode_id)?;
            let entries = self.list_directory(&inode);
            
            let mut found = entries.iter().find(|(name, _)| name == *part).map(|(_, id)| *id);
            
            // Special Case: If we are at root and the part is "data", skip it.
            // This allows absolute paths like "/data/file" to work within the ext2 mount.
            if found.is_none() && current_inode_id == 2 && *part == "data" {
                continue;
            }

            if let Some(next_id) = found {
                let next_inode = self.read_inode(next_id)?;
                
                // Check if this part is a symlink
                if (next_inode.i_mode & 0xA000) == 0xA000 {
                    let mut target = String::new();
                    if next_inode.i_size < 60 {
                        // Fast Symlink: Copy i_block to local to avoid unaligned reference
                        let i_block = next_inode.i_block;
                        let ptr = i_block.as_ptr() as *const u8;
                        let slice = unsafe { core::slice::from_raw_parts(ptr, next_inode.i_size as usize) };
                        if let Ok(s) = core::str::from_utf8(slice) {
                            target = String::from(s);
                        }
                    } else {
                        let data = self.read_inode_data(&next_inode);
                        if let Ok(s) = core::str::from_utf8(&data) {
                            target = String::from(s);
                        }
                    }

                    if target.is_empty() { return None; }

                    // Resolve the symlink target
                    // Combine target + remaining parts
                    let mut full_new_path = target;
                    if i + 1 < parts.len() {
                        if !full_new_path.ends_with('/') { full_new_path.push('/'); }
                        full_new_path.push_str(&parts[i+1..].join("/"));
                    }
                    
                    // Recursive call: if target is absolute, it starts from root, otherwise from parent (current_inode_id)
                    return self.resolve_path_recursive(&full_new_path, current_inode_id, depth + 1);
                }
                
                current_inode_id = next_id;
            } else { return None; }
        }
        Some(current_inode_id)
    }

    pub fn get_block_id(&self, inode: &Inode, logical_block: u32) -> Option<u32> {
        let block_size = self.inner.lock().block_size;
        let ptrs_per_block = block_size / 4;
        if logical_block < 12 { return Some(inode.i_block[logical_block as usize]); }
        
        let mut index = logical_block - 12;
        if index < ptrs_per_block {
            let indir_block = inode.i_block[12];
            if indir_block == 0 { return Some(0); } // Not allocated yet
            let data = self.read_block(indir_block)?;
            let ptrs = unsafe { core::slice::from_raw_parts(data.as_ptr() as *const u32, ptrs_per_block as usize) };
            return Some(ptrs[index as usize]);
        }
        
        index -= ptrs_per_block;
        let ptrs_per_double = ptrs_per_block * ptrs_per_block;
        if index < ptrs_per_double {
            let d_indir_block = inode.i_block[13];
            if d_indir_block == 0 { return Some(0); }
            let data1 = self.read_block(d_indir_block)?;
            let ptrs1 = unsafe { core::slice::from_raw_parts(data1.as_ptr() as *const u32, ptrs_per_block as usize) };
            let i1 = index / ptrs_per_block;
            let i2 = index % ptrs_per_block;
            let indir_block2 = ptrs1[i1 as usize];
            if indir_block2 == 0 { return Some(0); }
            let data2 = self.read_block(indir_block2)?;
            let ptrs2 = unsafe { core::slice::from_raw_parts(data2.as_ptr() as *const u32, ptrs_per_block as usize) };
            return Some(ptrs2[i2 as usize]);
        }
        
        index -= ptrs_per_double;
        let ptrs_per_triple = ptrs_per_double * ptrs_per_block;
        if index < ptrs_per_triple {
            let t_indir_block = inode.i_block[14];
            if t_indir_block == 0 { return Some(0); }
            let data1 = self.read_block(t_indir_block)?;
            let ptrs1 = unsafe { core::slice::from_raw_parts(data1.as_ptr() as *const u32, ptrs_per_block as usize) };
            
            let i1 = index / ptrs_per_double;
            let i2 = (index % ptrs_per_double) / ptrs_per_block;
            let i3 = index % ptrs_per_block;
            
            let indir2 = ptrs1[i1 as usize];
            if indir2 == 0 { return Some(0); }
            let data2 = self.read_block(indir2)?;
            let ptrs2 = unsafe { core::slice::from_raw_parts(data2.as_ptr() as *const u32, ptrs_per_block as usize) };
            
            let indir3 = ptrs2[i2 as usize];
            if indir3 == 0 { return Some(0); }
            let data3 = self.read_block(indir3)?;
            let ptrs3 = unsafe { core::slice::from_raw_parts(data3.as_ptr() as *const u32, ptrs_per_block as usize) };
            
            return Some(ptrs3[i3 as usize]);
        }

        None
    }

    fn ensure_block_id(&self, inode_id: u32, logical_block: u32) -> Option<u32> {
        let mut inode = self.read_inode(inode_id)?;
        let block_size = self.inner.lock().block_size;
        let ptrs_per_block = block_size / 4;
        
        if logical_block < 12 {
            if inode.i_block[logical_block as usize] != 0 {
                return Some(inode.i_block[logical_block as usize]);
            }
            let new_block = self.allocate_block()?;
            inode.i_block[logical_block as usize] = new_block;
            self.write_inode(inode_id, &inode);
            return Some(new_block);
        }

        let mut index = logical_block - 12;
        if index < ptrs_per_block {
            if inode.i_block[12] == 0 {
                inode.i_block[12] = self.allocate_block()?;
                self.write_inode(inode_id, &inode);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(inode.i_block[12], &empty);
            }
            let indir_block = inode.i_block[12];
            let mut data = self.read_block(indir_block)?;
            let ptrs = unsafe { core::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs[index as usize] != 0 { return Some(ptrs[index as usize]); }
            
            let new_block = self.allocate_block()?;
            ptrs[index as usize] = new_block;
            self.write_block(indir_block, &data);
            return Some(new_block);
        }

        index -= ptrs_per_block;
        let ptrs_per_double = ptrs_per_block * ptrs_per_block;
        if index < ptrs_per_double {
            if inode.i_block[13] == 0 {
                inode.i_block[13] = self.allocate_block()?;
                self.write_inode(inode_id, &inode);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(inode.i_block[13], &empty);
            }
            let d_indir = inode.i_block[13];
            let i1 = index / ptrs_per_block;
            let i2 = index % ptrs_per_block;
            
            let mut data1 = self.read_block(d_indir)?;
            let ptrs1 = unsafe { core::slice::from_raw_parts_mut(data1.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs1[i1 as usize] == 0 {
                ptrs1[i1 as usize] = self.allocate_block()?;
                self.write_block(d_indir, &data1);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(ptrs1[i1 as usize], &empty);
            }
            
            let indir2 = ptrs1[i1 as usize];
            let mut data2 = self.read_block(indir2)?;
            let ptrs2 = unsafe { core::slice::from_raw_parts_mut(data2.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs2[i2 as usize] != 0 { return Some(ptrs2[i2 as usize]); }
            
            let new_block = self.allocate_block()?;
            ptrs2[i2 as usize] = new_block;
            self.write_block(indir2, &data2);
            return Some(new_block);
        }

        index -= ptrs_per_double;
        let ptrs_per_triple = ptrs_per_double * ptrs_per_block;
        if index < ptrs_per_triple {
            if inode.i_block[14] == 0 {
                inode.i_block[14] = self.allocate_block()?;
                self.write_inode(inode_id, &inode);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(inode.i_block[14], &empty);
            }
            let t_indir = inode.i_block[14];
            let i1 = index / ptrs_per_double;
            let i2 = (index % ptrs_per_double) / ptrs_per_block;
            let i3 = index % ptrs_per_block;

            let mut data1 = self.read_block(t_indir)?;
            let ptrs1 = unsafe { core::slice::from_raw_parts_mut(data1.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs1[i1 as usize] == 0 {
                ptrs1[i1 as usize] = self.allocate_block()?;
                self.write_block(t_indir, &data1);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(ptrs1[i1 as usize], &empty);
            }

            let indir2 = ptrs1[i1 as usize];
            let mut data2 = self.read_block(indir2)?;
            let ptrs2 = unsafe { core::slice::from_raw_parts_mut(data2.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs2[i2 as usize] == 0 {
                ptrs2[i2 as usize] = self.allocate_block()?;
                self.write_block(indir2, &data2);
                let empty = alloc::vec![0u8; block_size as usize];
                self.write_block(ptrs2[i2 as usize], &empty);
            }

            let indir3 = ptrs2[i2 as usize];
            let mut data3 = self.read_block(indir3)?;
            let ptrs3 = unsafe { core::slice::from_raw_parts_mut(data3.as_mut_ptr() as *mut u32, ptrs_per_block as usize) };
            if ptrs3[i3 as usize] != 0 { return Some(ptrs3[i3 as usize]); }

            let new_block = self.allocate_block()?;
            ptrs3[i3 as usize] = new_block;
            self.write_block(indir3, &data3);
            return Some(new_block);
        }

        None
    }

    pub fn read_inode_data(&self, inode: &Inode) -> Vec<u8> {
        let block_size = self.inner.lock().block_size;
        let mut data = Vec::new();
        let mut bytes_left = inode.i_size as usize;
        let total_blocks = (inode.i_size + block_size - 1) / block_size;
        for i in 0..total_blocks {
            if bytes_left == 0 { break; }
            if let Some(block_id) = self.get_block_id(inode, i) {
                if block_id == 0 {
                    let to_copy = core::cmp::min(bytes_left, block_size as usize);
                    data.extend(core::iter::repeat(0).take(to_copy));
                    bytes_left -= to_copy;
                } else if let Some(block_data) = self.read_block(block_id) {
                    let to_copy = core::cmp::min(bytes_left, block_data.len());
                    data.extend_from_slice(&block_data[..to_copy]);
                    bytes_left -= to_copy;
                } else { break; }
            } else { break; }
        }
        data
    }

    fn get_or_create_inode_with_owner(&self, path: &str, uid: u16, gid: u16) -> Option<u32> {
        if let Some(id) = self.resolve_path(path) { return Some(id); }
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return None; }
        let filename = parts.last().unwrap();
        let parent_path = if parts.len() == 1 { "/" } else { &path[..path.len() - filename.len() - 1] };
        let parent_inode_id = self.resolve_path(parent_path)?;
        let new_inode_id = self.allocate_inode()?;
        let mut inode = Inode {
            i_mode: 0x81A4, i_uid: uid, i_gid: gid, i_size: 0, i_atime: 0, i_ctime: 0, i_mtime: 0, i_dtime: 0,
            i_links_count: 1, i_blocks: 0, i_flags: 0, i_osd1: 0, i_block: [0; 15],
            i_generation: 0, i_file_acl: 0, i_dir_acl: 0, i_faddr: 0, i_osd2: [0; 12],
        };
        self.write_inode(new_inode_id, &inode);
        if !self.add_entry(parent_inode_id, filename, new_inode_id, 1) { return None; }
        Some(new_inode_id)
    }
}

impl FileSystem for Ext2Fs {
    fn create_dir(&self, path: &str, uid: u16, gid: u16) -> bool {
        // TODO: Check parent directory write permission
        if self.resolve_path(path).is_some() { return false; } // Already exists
        
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return false; }
        let dirname = parts.last().unwrap();
        let parent_path = if parts.len() == 1 { "/" } else { &path[..path.len() - dirname.len() - 1] };
        
        let parent_inode_id = match self.resolve_path(parent_path) {
            Some(id) => id,
            None => return false,
        };

        // 1. Allocate inode
        let new_inode_id = match self.allocate_inode() {
            Some(id) => id,
            None => return false,
        };

        // 2. Allocate data block for . and ..
        let block_id = match self.allocate_block() {
            Some(id) => id,
            None => {
                self.deallocate_inode(new_inode_id);
                return false;
            }
        };

        let block_size = self.inner.lock().block_size;
        let mut data = alloc::vec![0u8; block_size as usize];
        
        // Setup "."
        let de_dot = unsafe { &mut *(data.as_mut_ptr() as *mut DirEntry) };
        de_dot.inode = new_inode_id;
        de_dot.rec_len = 12;
        de_dot.name_len = 1;
        de_dot.file_type = 2; // Directory
        data[8] = b'.';

        // Setup ".."
        let de_dotdot = unsafe { &mut *(data[12..].as_mut_ptr() as *mut DirEntry) };
        de_dotdot.inode = parent_inode_id;
        de_dotdot.rec_len = (block_size - 12) as u16;
        de_dotdot.name_len = 2;
        de_dotdot.file_type = 2;
        data[12 + 8] = b'.';
        data[12 + 9] = b'.';

        if !self.write_block(block_id, &data) { return false; }

        // 3. Initialize Inode
        let mut inode = Inode {
            i_mode: 0x41ED, // Directory, 755
            i_uid: uid, i_gid: gid, i_size: block_size, i_atime: 0, i_ctime: 0, i_mtime: 0, i_dtime: 0,
            i_links_count: 2, i_blocks: (block_size / 512) as u32, i_flags: 0, i_osd1: 0, i_block: [0; 15],
            i_generation: 0, i_file_acl: 0, i_dir_acl: 0, i_faddr: 0, i_osd2: [0; 12],
        };
        inode.i_block[0] = block_id;
        
        if !self.write_inode(new_inode_id, &inode) { return false; }

        // 4. Add to parent
        if !self.add_entry(parent_inode_id, dirname, new_inode_id, 2) { return false; }

        self.sync_metadata();
        true
    }

    fn read_file(&self, path: &str, uid: u16, gid: u16) -> Option<Vec<u8>> {
        let inode_id = self.resolve_path(path)?;
        let inode = self.read_inode(inode_id)?;
        if !self.check_access(&inode, 4, uid, gid) { return None; }
        Some(self.read_inode_data(&inode))
    }

    fn write_file(&self, path: &str, data: &[u8], uid: u16, gid: u16) -> bool {
        let inode_id = match self.resolve_path(path) {
            Some(id) => {
                let inode = self.read_inode(id).unwrap();
                if !self.check_access(&inode, 2, uid, gid) { return false; }
                id
            },
            None => {
                match self.get_or_create_inode_with_owner(path, uid, gid) {
                    Some(id) => id,
                    None => return false,
                }
            }
        };
        
        // Read current inode to get existing blocks
        let mut inode = match self.read_inode(inode_id) {
            Some(i) => i,
            None => return false,
        };

        let block_size = self.inner.lock().block_size;
        let mut bytes_written = 0;
        let mut block_idx = 0;

        while bytes_written < data.len() {
            let block_id = match self.ensure_block_id(inode_id, block_idx) {
                Some(id) => id,
                None => return false,
            };
            
            // Re-read inode because ensure_block_id might have updated it
            inode = self.read_inode(inode_id).unwrap();

            let to_write = core::cmp::min(data.len() - bytes_written, block_size as usize);
            let mut block_data = alloc::vec![0u8; block_size as usize];
            block_data[..to_write].copy_from_slice(&data[bytes_written..bytes_written + to_write]);
            
            if !self.write_block(block_id, &block_data) {
                return false;
            }
            
            bytes_written += to_write;
            block_idx += 1;
        }

        // Update metadata
        inode.i_size = data.len() as u32;
        inode.i_blocks = (block_idx * block_size / 512) as u32;
        inode.i_mtime += 1;
        
        if !self.write_inode(inode_id, &inode) {
            return false;
        }
        
        self.sync_metadata();
        true
    }

    fn delete_file(&self, path: &str, uid: u16, _gid: u16) -> bool {
        let inode_id = match self.resolve_path(path) {
            Some(id) => id,
            None => return false,
        };

        let inode = match self.read_inode(inode_id) {
            Some(i) => i,
            None => return false,
        };

        // Only owner or root can delete
        if uid != 0 && uid != inode.i_uid { return false; }

        // 1. Deallocate all data blocks
        let block_size = self.inner.lock().block_size;
        let total_blocks = (inode.i_size + block_size - 1) / block_size;
        for i in 0..total_blocks {
            if let Some(block_id) = self.get_block_id(&inode, i) {
                if block_id != 0 {
                    self.deallocate_block(block_id);
                }
            }
        }
        // TODO: Handle deallocating indirect block pointers themselves

        // 2. Deallocate the inode
        self.deallocate_inode(inode_id);

        // 3. Remove entry from parent directory
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return false; }
        let filename = parts.last().unwrap();
        let parent_path = if parts.len() == 1 { "/" } else { &path[..path.len() - filename.len() - 1] };
        
        let parent_inode_id = match self.resolve_path(parent_path) {
            Some(id) => id,
            None => return false,
        };

        if !self.remove_entry(parent_inode_id, filename) {
            return false;
        }

        self.sync_metadata();
        true
    }
    fn create_symlink(&self, path: &str, target: &str, uid: u16, gid: u16) -> bool {
        if self.resolve_path(path).is_some() { return false; } // Already exists
        
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return false; }
        let filename = parts.last().unwrap();
        let parent_path = if parts.len() == 1 { "/" } else { &path[..path.len() - filename.len() - 1] };
        
        let parent_inode_id = match self.resolve_path(parent_path) {
            Some(id) => id,
            None => return false,
        };

        // 1. Allocate inode
        let new_inode_id = match self.allocate_inode() {
            Some(id) => id,
            None => return false,
        };

        let mut inode = Inode {
            i_mode: 0xA1FF, // Symlink + 777
            i_uid: uid, i_gid: gid, i_size: target.len() as u32,
            i_atime: 0, i_ctime: 0, i_mtime: 0, i_dtime: 0,
            i_links_count: 1, i_blocks: 0, i_flags: 0, i_osd1: 0, i_block: [0; 15],
            i_generation: 0, i_file_acl: 0, i_dir_acl: 0, i_faddr: 0, i_osd2: [0; 12],
        };

        // 2. Fast vs Slow Symlink
        if target.len() < 60 {
            let mut i_block = [0u32; 15];
            unsafe { core::ptr::copy_nonoverlapping(target.as_ptr(), i_block.as_mut_ptr() as *mut u8, target.len()); }
            inode.i_block = i_block;
        } else {
            let mut i = 0;
            while i * 4096 < target.len() {
                if let Some(block_id) = self.ensure_block_id(new_inode_id, i as u32) {
                    let start = i * 4096;
                    let end = core::cmp::min(start + 4096, target.len());
                    self.write_block(block_id, &target.as_bytes()[start..end]);
                } else {
                    self.deallocate_inode(new_inode_id);
                    return false;
                }
                i += 1;
            }
            if let Some(i) = self.read_inode(new_inode_id) { inode = i; } else { return false; }
            inode.i_mode = 0xA1FF; inode.i_size = target.len() as u32;
        }

        self.write_inode(new_inode_id, &inode);

        if !self.add_entry(parent_inode_id, filename, new_inode_id, 7) { 
            self.deallocate_inode(new_inode_id);
            return false;
        }

        self.sync_metadata();
        true
    }

    fn rename_file(&self, old_path: &str, new_path: &str, uid: u16, _gid: u16) -> bool {
        let inode_id = match self.resolve_path(old_path) {
            Some(id) => id,
            None => return false,
        };
        let inode = self.read_inode(inode_id).unwrap();
        if uid != 0 && uid != inode.i_uid { return false; }

        let file_type = if (inode.i_mode & 0x4000) != 0 { 2 } else { 1 };
        if self.resolve_path(new_path).is_some() {
            if !self.delete_file(new_path, uid, 0) { return false; }
        }

        let new_parts: Vec<&str> = new_path.split('/').filter(|s| !s.is_empty()).collect();
        let (new_parent_path, new_filename) = if new_parts.is_empty() { ("/", "") } else {
            let fname = new_parts.last().unwrap();
            let p_path = if new_parts.len() == 1 { "/" } else { &new_path[..new_path.len() - fname.len() - 1] };
            (p_path, *fname)
        };
        
        let new_parent_inode_id = self.resolve_path(new_parent_path).unwrap();
        if !self.add_entry(new_parent_inode_id, new_filename, inode_id, file_type) { return false; }

        let old_parts: Vec<&str> = old_path.split('/').filter(|s| !s.is_empty()).collect();
        let (old_parent_path, old_filename) = if old_parts.is_empty() { ("/", "") } else {
            let fname = old_parts.last().unwrap();
            let p_path = if old_parts.len() == 1 { "/" } else { &old_path[..old_path.len() - fname.len() - 1] };
            (p_path, *fname)
        };
        
        let old_parent_inode_id = self.resolve_path(old_parent_path).unwrap();
        if !self.remove_entry(old_parent_inode_id, old_filename) { return false; }

        self.sync_metadata();
        true
    }

    fn list_dir(&self, path: &str, _uid: u16, _gid: u16) -> Vec<String> {
        if let Some(inode_id) = self.resolve_path(path) {
            if let Some(inode) = self.read_inode(inode_id) {
                // TODO: Check dir read/exec permission
                return self.list_directory(&inode).into_iter().map(|(name, _)| name).collect();
            }
        }
        Vec::new()
    }

    fn chmod(&self, path: &str, mode: u16, uid: u16, _gid: u16) -> bool {
        if let Some(inode_id) = self.resolve_path(path) {
            if let Some(inode) = self.read_inode(inode_id) {
                // Only owner or root can chmod
                if uid != 0 && uid != inode.i_uid { return false; }
                
                // Keep the file type bits (upper 4 bits)
                let mut inode_mut = inode;
                let file_type = inode_mut.i_mode & 0xF000;
                inode_mut.i_mode = file_type | (mode & 0x0FFF);
                if self.write_inode(inode_id, &inode_mut) {
                    self.sync_metadata();
                    return true;
                }
            }
        }
        false
    }

    fn chown(&self, path: &str, new_uid: u16, new_gid: u16, uid: u16, _gid: u16) -> bool {
        if let Some(inode_id) = self.resolve_path(path) {
            if let Some(inode) = self.read_inode(inode_id) {
                // Only root can chown
                if uid != 0 { return false; }
                
                let mut inode_mut = inode;
                inode_mut.i_uid = new_uid;
                inode_mut.i_gid = new_gid;
                if self.write_inode(inode_id, &inode_mut) {
                    self.sync_metadata();
                    return true;
                }
            }
        }
        false
    }
}

impl Ext2Fs {
    pub fn format(drive_id: u8, start_lba: u32, num_sectors: u32) -> bool {
        let block_size: u32 = 4096;
        let sectors_per_block = block_size / 512;
        let total_blocks = num_sectors / sectors_per_block;
        
        // Basic Ext2 defaults
        let inodes_per_group = 2048;
        let blocks_per_group = 8192;
        let group_count = (total_blocks + blocks_per_group - 1) / blocks_per_group;
        let total_inodes = group_count * inodes_per_group;

        // 1. Create Superblock
        let mut sb = Superblock {
            s_inodes_count: total_inodes,
            s_blocks_count: total_blocks,
            s_r_blocks_count: 0,
            s_free_blocks_count: total_blocks, // Will adjust below
            s_free_inodes_count: total_inodes - 11, // First 10 are reserved
            s_first_data_block: if block_size == 1024 { 1 } else { 0 },
            s_log_block_size: 2, // 4096
            s_log_frag_size: 0,
            s_blocks_per_group: blocks_per_group,
            s_frags_per_group: blocks_per_group,
            s_inodes_per_group: inodes_per_group,
            s_mtime: 0, s_wtime: 0, s_mnt_count: 0, s_max_mnt_count: 50,
            s_magic: 0xEF53,
            s_state: 1, s_errors: 1, s_minor_rev_level: 0,
            s_lastcheck: 0, s_checkinterval: 0, s_creator_os: 0, s_rev_level: 1,
            s_def_resuid: 0, s_def_resgid: 0,
            s_first_ino: 11,
            s_inode_size: 128,
            s_block_group_nr: 0,
            s_feature_compat: 0, s_feature_incompat: 0x2,
            s_feature_ro_compat: 0,
            s_uuid: [0; 16], s_volume_name: [0; 16], s_last_mounted: [0; 64],
            s_algo_bitmap: 0,
            _reserved: [0; 812],
        };

        // 2. Prepare Block Group Descriptors
        let mut groups = alloc::vec![BlockGroupDescriptor {
            bg_block_bitmap: 0, bg_inode_bitmap: 0, bg_inode_table: 0,
            bg_free_blocks_count: blocks_per_group as u16,
            bg_free_inodes_count: inodes_per_group as u16,
            bg_used_dirs_count: 0, bg_pad: 0, _reserved: [0; 12],
        }; group_count as usize];

        let bgdt_blocks = (group_count * 32 + block_size - 1) / block_size;
        let mut current_block = sb.s_first_data_block + (1024 / block_size) + bgdt_blocks;
        if block_size > 1024 { current_block = 1 + bgdt_blocks; }

        for g in 0..group_count as usize {
            groups[g].bg_block_bitmap = current_block;
            groups[g].bg_inode_bitmap = current_block + 1;
            groups[g].bg_inode_table = current_block + 2;
            let itable_blocks = (inodes_per_group * 128 + block_size - 1) / block_size;
            
            let metadata_blocks = 2 + itable_blocks;
            groups[g].bg_free_blocks_count -= metadata_blocks as u16;
            sb.s_free_blocks_count -= metadata_blocks;

            current_block += metadata_blocks;
            
            if g == 0 {
                groups[g].bg_free_inodes_count -= 10; // Reserved
                groups[g].bg_used_dirs_count = 1; // Root
            }
        }

        // 3. Initialize Root Directory Inode (Inode 2)
        let root_dir_block = current_block;
        current_block += 1;
        groups[0].bg_free_blocks_count -= 1;
        sb.s_free_blocks_count -= 1;
        
        let root_inode = Inode {
            i_mode: 0x41ED, // Directory, 755
            i_uid: 0, i_size: block_size, i_atime: 0, i_ctime: 0, i_mtime: 0, i_dtime: 0,
            i_gid: 0, i_links_count: 2, i_blocks: sectors_per_block, i_flags: 0, i_osd1: 0, 
            i_block: {
                let mut b = [0u32; 15];
                b[0] = root_dir_block;
                b
            },
            i_generation: 0, i_file_acl: 0, i_dir_acl: 0, i_faddr: 0, i_osd2: [0; 12],
        };

        // 4. Create Root Directory Entries
        let mut root_data = alloc::vec![0u8; block_size as usize];
        let de_dot = unsafe { &mut *(root_data.as_mut_ptr() as *mut DirEntry) };
        de_dot.inode = 2; de_dot.rec_len = 12; de_dot.name_len = 1; de_dot.file_type = 2;
        root_data[8] = b'.';
        let de_dotdot = unsafe { &mut *(root_data[12..].as_mut_ptr() as *mut DirEntry) };
        de_dotdot.inode = 2; de_dotdot.rec_len = (block_size - 12) as u16; de_dotdot.name_len = 2; de_dotdot.file_type = 2;
        root_data[20] = b'.'; root_data[21] = b'.';

        // 5. Setup Bitmaps for Group 0
        let mut b_bitmap = alloc::vec![0u8; block_size as usize];
        let mut i_bitmap = alloc::vec![0u8; block_size as usize];
        
        // Mark metadata blocks used in group 0
        let metadata_count = (root_dir_block - sb.s_first_data_block) as usize;
        for i in 0..metadata_count + 1 {
            b_bitmap[i / 8] |= 1 << (i % 8);
        }
        // Mark reserved inodes used
        for i in 0..11 {
            i_bitmap[i / 8] |= 1 << (i % 8);
        }

        // 6. Write everything to disk
        // Superblock
        let mut sb_buf = [0u8; 1024];
        unsafe { core::ptr::write_unaligned(sb_buf.as_mut_ptr() as *mut Superblock, sb); }
        crate::disk::write_sectors(drive_id, start_lba + 2, 2, &sb_buf).ok();

        // BGDT
        let bgdt_lba = start_lba + (if block_size == 1024 { 2 } else { 1 }) * sectors_per_block;
        let mut bgdt_data = alloc::vec![0u8; (bgdt_blocks * block_size) as usize];
        for (i, desc) in groups.iter().enumerate() {
            unsafe { core::ptr::write_unaligned(bgdt_data[i*32..].as_mut_ptr() as *mut BlockGroupDescriptor, *desc); }
        }
        crate::disk::write_sectors(drive_id, bgdt_lba, (bgdt_blocks * sectors_per_block) as u8, &bgdt_data).ok();

        // Bitmaps and Root Inode
        crate::disk::write_sectors(drive_id, start_lba + groups[0].bg_block_bitmap * sectors_per_block, sectors_per_block as u8, &b_bitmap).ok();
        crate::disk::write_sectors(drive_id, start_lba + groups[0].bg_inode_bitmap * sectors_per_block, sectors_per_block as u8, &i_bitmap).ok();
        
        // Inode Table (Root Inode is at index 1 in the table, since inode 1 is reserved and index 0)
        let mut itable_buf = alloc::vec![0u8; block_size as usize];
        unsafe { core::ptr::write_unaligned(itable_buf[128..].as_mut_ptr() as *mut Inode, root_inode); }
        crate::disk::write_sectors(drive_id, start_lba + groups[0].bg_inode_table * sectors_per_block, sectors_per_block as u8, &itable_buf).ok();

        // Root Directory Data
        crate::disk::write_sectors(drive_id, start_lba + root_dir_block * sectors_per_block, sectors_per_block as u8, &root_data).ok();

        crate::serial_println!("EXT2: Formatting drive {} at LBA {} ({} sectors)... DONE", drive_id, start_lba, num_sectors);
        true
    }
}
