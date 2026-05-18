# Implementation Plan: ext2 Filesystem for MYNEWOS

This plan outlines the transition from a hardcoded FAT32 driver to a multi-filesystem architecture supporting **ext2**, a standard Linux filesystem.

## Phase 1: Virtual File System (VFS) Abstraction (DONE)
- [x] **Define `Trait FileSystem`**: Create a Rust trait for common operations.
- [x] **Global VFS Registry**: A static map that associates path prefixes (e.g., `/data`) with specific filesystem instances.
- [x] **Refactor Syscalls**: Update `sys_open`, `sys_read`, etc., to use the VFS.

## Phase 2: ext2 Core Structures (DONE)
- [x] **Superblock Parser**: Read the superblock at offset 1024.
- [x] **Block Group Descriptor Table (BGDT)**: Used to find the inode tables.
- [x] **Inode Parser**: Successfully reading 128-byte (Rev 0) and dynamic-sized inodes.

## Phase 3: Directory and File Traversal (DONE)
- [x] **Root Directory**: Start at Inode 2.
- [x] **Directory Entry Parser**: Parses variable-length `ext2_dir_entry` structures.
- [x] **Path Resolution**: Supports recursive lookup (e.g., `/data/hello.txt`).

## Phase 4: Data Block Addressing (DONE)
- [x] **Direct Blocks**: Handle the first 12 blocks.
- [x] **Indirect Blocks**: Single indirect block support.
- [x] **Double Indirect**: Support for larger files (implemented).
- [x] **Sparse Files**: Handle block pointers that are 0 (fill with zeros).

## Phase 5: Build System & Integration (DONE)
- [x] **Secondary Disk Support**: ATA driver updated to support Master/Slave disks.
- [x] **Automated Population**: `Makefile` uses `mke2fs -d` to inject files into the ext2 image at build time.
- [x] **VFS Routing**: Correctly handles `/` (FAT32) and `/data` (ext2).

## Phase 6: Write Support (DONE)
- [x] **Block Bitmap**: `allocate_block` and `deallocate_block` implemented.
- [x] **Inode Bitmap**: `allocate_inode` and `deallocate_inode` implemented.
- [x] **New File Creation**: `get_or_create_inode` and `add_entry` implemented.
- [x] **Data Writing**: `write_file` with block allocation logic.
- [x] **File Deletion**: `delete_file` and `remove_entry` implemented.
- [x] **Renaming**: `rename_file` implemented.

## Phase 7: Optimization & Robustness (In Progress)
- [x] **Block Cache**: Implement an LRU-style cache for disk sectors (Write-Through).
- [x] **Inode Cache**: Cache frequently used inodes to avoid table lookups.
- [x] **Triple Indirect Support**: Support for files larger than ~16GB.
- [ ] **Partition Table Parser**: Support MBR/GPT for multi-partition disks.

## Phase 8: Advanced Features & System Integration
- [x] **Symbolic Links (Symlinks)**: Support for soft links and path redirection. (DONE)
- [x] **Kernel-side Formatting (mkfs)**: Ability to format a raw disk as Ext2 from the shell. (DONE)
- [x] **Permissions Enforcement**: Enforce UID/GID and mode bits (read/write/execute) in the VFS. (DONE)
- [ ] **Fragmentation Analysis**: Tool to check filesystem health/fragmentation. (NEXT)
- [ ] **Ext4 Extent Support**: Basic read-only support for modern Ext4 extent-based files.

---

> [!TIP]
> **Verification**: Use `ls /data` and `cat /data/hello.txt` in the MYNEWOS shell to verify current read-only support.
