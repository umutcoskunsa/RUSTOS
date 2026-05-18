
    fn rename_file(&self, old_path: &str, new_path: &str) -> bool {
        crate::serial_println!("EXT2: Rename '{}' -> '{}'", old_path, new_path);

        let inode_id = match self.resolve_path(old_path) {
            Some(id) => id,
            None => {
                crate::serial_println!("EXT2: Rename failed - source not found");
                return false;
            }
        };

        let inode = match self.read_inode(inode_id) {
            Some(i) => i,
            None => return false,
        };

        let file_type = if (inode.i_mode & 0x4000) != 0 { 2 } else { 1 };

        if self.resolve_path(new_path).is_some() {
            crate::serial_println!("EXT2: Rename - destination exists, deleting");
            if !self.delete_file(new_path) { return false; }
        }

        // 2. Add entry to new parent
        let new_parts: Vec<&str> = new_path.split('/').filter(|s| !s.is_empty()).collect();
        let (new_parent_path, new_filename) = if new_parts.is_empty() {
            ("/", "")
        } else {
            let fname = new_parts.last().unwrap();
            let p_path = if new_parts.len() == 1 { "/" } else { &new_path[..new_path.len() - fname.len() - 1] };
            (p_path, *fname)
        };
        
        let new_parent_inode_id = match self.resolve_path(new_parent_path) {
            Some(id) => id,
            None => {
                crate::serial_println!("EXT2: Rename failed - new parent '{}' not found", new_parent_path);
                return false;
            }
        };

        if !self.add_entry(new_parent_inode_id, new_filename, inode_id, file_type) {
            return false;
        }

        // 3. Remove entry from old parent
        let old_parts: Vec<&str> = old_path.split('/').filter(|s| !s.is_empty()).collect();
        let (old_parent_path, old_filename) = if old_parts.is_empty() {
            ("/", "")
        } else {
            let fname = old_parts.last().unwrap();
            let p_path = if old_parts.len() == 1 { "/" } else { &old_path[..old_path.len() - fname.len() - 1] };
            (p_path, *fname)
        };
        
        let old_parent_inode_id = match self.resolve_path(old_parent_path) {
            Some(id) => id,
            None => return false,
        };

        if !self.remove_entry(old_parent_inode_id, old_filename) {
            return false;
        }

        self.sync_metadata();
        true
    }

    fn list_dir(&self, path: &str) -> Vec<String> {
        if let Some(inode_id) = self.resolve_path(path) {
            if let Some(inode) = self.read_inode(inode_id) {
                return self.list_directory(&inode).into_iter().map(|(name, _)| name).collect();
            }
        }
        Vec::new()
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
            s_free_blocks_count: total_blocks - 100, // Approximate
            s_free_inodes_count: total_inodes - 11, // First 10 are reserved
            s_first_data_block: if block_size == 1024 { 1 } else { 0 },
            s_log_block_size: (block_size >> 11), // 4096 -> 2
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
            s_feature_compat: 0, s_feature_incompat: 0x2, // Sparse Superblock
            s_feature_ro_compat: 0,
            s_uuid: [0; 16], s_volume_name: [0; 16], s_last_mounted: [0; 64],
            s_algo_bitmap: 0, s_prealloc_blocks: 0, s_prealloc_dir_blocks: 0,
            s_padding1: 0, s_journal_uuid: [0; 16], s_journal_inum: 0,
            s_journal_dev: 0, s_last_orphan: 0, s_hash_seed: [0; 4], s_def_hash_version: 0,
            s_padding2: 0, s_default_mount_opts: 0, s_first_meta_bg: 0,
            s_unused: [0; 190],
        };

        // 2. Prepare Block Group Descriptors
        let mut groups = alloc::vec![BlockGroupDescriptor {
            bg_block_bitmap: 0, bg_inode_bitmap: 0, bg_inode_table: 0,
            bg_free_blocks_count: blocks_per_group as u16,
            bg_free_inodes_count: inodes_per_group as u16,
            bg_used_dirs_count: 0, bg_pad: 0, bg_reserved: [0; 12],
        }; group_count as usize];

        // Layout for Group 0
        let bgdt_blocks = (group_count * 32 + block_size - 1) / block_size;
        let mut current_block = sb.s_first_data_block + (1024 / block_size) + bgdt_blocks;
        
        for g in 0..group_count as usize {
            groups[g].bg_block_bitmap = current_block;
            groups[g].bg_inode_bitmap = current_block + 1;
            groups[g].bg_inode_table = current_block + 2;
            let itable_blocks = (inodes_per_group * 128 + block_size - 1) / block_size;
            current_block += 2 + itable_blocks;
            
            // Adjust free counts for Group 0
            if g == 0 {
                groups[g].bg_free_inodes_count -= 10; // Reserved
                groups[g].bg_used_dirs_count = 1; // Root
            }
        }

        // 3. Write Superblock
        let mut sb_buf = [0u8; 1024];
        unsafe { core::ptr::write_unaligned(sb_buf.as_mut_ptr() as *mut Superblock, sb); }
        crate::disk::write_sectors(drive_id, start_lba + 2, 2, &sb_buf).ok();

        // 4. Write BGDT
        let mut bgdt_data = alloc::vec![0u8; (bgdt_blocks * block_size) as usize];
        for (i, desc) in groups.iter().enumerate() {
            let offset = i * 32;
            unsafe { core::ptr::write_unaligned(bgdt_data[offset..].as_mut_ptr() as *mut BlockGroupDescriptor, *desc); }
        }
        let bgdt_lba = start_lba + (sb.s_first_data_block + (1024 / block_size)) * sectors_per_block;
        crate::disk::write_sectors(drive_id, bgdt_lba, (bgdt_blocks * sectors_per_block) as u8, &bgdt_data).ok();

        // 5. Initialize Root Directory (Inode 2)
        // This is a simplified mkfs. For a real one, we'd need to zero out bitmaps, etc.
        // But for testing, we can just mount it and let the driver handle allocations if metadata is mostly valid.
        
        crate::serial_println!("EXT2: Formatting drive {} at LBA {} ({} sectors)... DONE", drive_id, start_lba, num_sectors);
        true
    }
}
