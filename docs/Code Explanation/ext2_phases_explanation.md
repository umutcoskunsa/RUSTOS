# Ext2 Filesystem Implementation: Phase-by-Phase Code Explanation

This document provides a comprehensive overview of what we have achieved in building the Ext2 filesystem for MYNEWOS, broken down by each phase of our implementation plan.

## Phase 1: Virtual File System (VFS) Abstraction
**Goal**: Create a generic abstraction layer that allows the kernel to interact with different filesystems (FAT32, Ext2) uniformly.

**Achievements**:
- **`FileSystem` Trait**: We defined a Rust trait (`kernel/src/fs/mod.rs`) containing standard operations like `read_file`, `write_file`, `create_dir`, `chmod`, and `chown`.
- **Global Mount Registry**: We implemented a thread-safe registry (`MOUNTS`) to map directory prefixes (e.g., `/` for FAT32, `/data` for Ext2) to their respective filesystem instances.
- **Context Routing**: Shell operations now simply call the global VFS API, which inspects the path, finds the longest matching mount point, strips the prefix, and routes the call to the appropriate driver (Ext2 or FAT32) while automatically passing the active `CURRENT_UID` and `CURRENT_GID`.

## Phase 2: Core Ext2 Structures
**Goal**: Accurately parse the fundamental on-disk structures of the Ext2 filesystem.

**Achievements**:
- **Superblock (`kernel/src/fs/ext2.rs`)**: Modeled the Superblock (at offset 1024), extracting essential metadata such as block sizes, inodes count, blocks per group, and the magic signature (`0xEF53`).
- **Block Group Descriptor Table (BGDT)**: Computed the location of the BGDT to locate crucial metadata, specifically the Inode Tables and Bitmaps for block groups.
- **Inode Parser**: Built a robust struct representation for Inodes. We accounted for dynamically sized inodes and handled safe, unaligned memory accesses when parsing binary disk structures.

## Phase 3: Directory and File Traversal
**Goal**: Navigate the Ext2 directory tree from the root.

**Achievements**:
- **Root Directory Anchoring**: Bootstrapped the traversal by reading Inode 2, which is standardized as the root directory in Ext2.
- **Directory Entry Parser**: Read the data blocks of directory inodes as arrays of `ext2_dir_entry` structures. Handled variable-length entries correctly, reading the `rec_len` and converting the character sequences into standard Rust `String` instances.
- **Recursive Lookup**: Implemented `resolve_path`, allowing multi-tiered paths (like `/dir/subdir/file.txt`) to be iteratively parsed into their final Inode ID.

## Phase 4: Data Block Addressing
**Goal**: Locate the actual file data spread across the disk.

**Achievements**:
- **Direct Blocks**: Fetched the first 12 direct block pointers (`i_block[0..11]`).
- **Indirect Blocks**: Successfully parsed single, double, and triple indirect blocks, chaining through block pointers to retrieve arbitrarily large chunks of file data.
- **Sparse Files Support**: Included checks for block IDs of `0`, returning zeroed memory instead of attempting to read from disk, thus supporting sparse files gracefully.

## Phase 5: Build System & Integration
**Goal**: Enable seamless kernel and user testing with Ext2.

**Achievements**:
- **ATA Drive Expansion**: Upgraded the generic ATA disk driver to support polling master and slave disk interfaces on identical buses.
- **`mke2fs` Integration**: Injected a build step in the `Makefile` to generate and populate a 10MB raw `.img` file formatted with Ext2, which is then attached to QEMU as Disk 1.

## Phase 6: Write Support (Data Modification)
**Goal**: Allow modifying, creating, and deleting Ext2 files and directories.

**Achievements**:
- **Bitmap Management**: Added functionality to read, toggle, and write back the Block and Inode Bitmaps, tracking free and allocated resources.
- **Resource Allocation**: `allocate_block` and `allocate_inode` were introduced to claim the first free bit from the bitmaps and properly update counters in the Superblock and Block Group Descriptors.
- **File System Mutation**: Implemented fully functional `write_file`, `create_dir`, `delete_file`, and `rename_file`. We made sure directory entries are properly added/removed and that `sync_metadata` pushes our cached superblock/BGDT back to the disk.

## Phase 7: Optimization & Robustness
**Goal**: Reduce disk reads and significantly improve performance.

**Achievements**:
- **LRU Block Cache**: Placed an abstraction around `disk::read_sectors` and `disk::write_sectors` within Ext2. It caches up to a fixed number of recent disk blocks in RAM. All reads consult the cache first, and writes employ a Write-Through policy to ensure integrity.
- **Inode Cache**: Added a HashMap to store recently loaded Inodes, eliminating the need to frequently calculate and load the sector containing the Inode Table.

## Phase 8: Advanced Features & Security
**Goal**: Make the filesystem robust, feature-complete, and multi-user compliant.

**Achievements**:
- **Symbolic Links**: 
  - **Fast Symlinks**: Small target paths (< 60 chars) are stored directly inside the `i_block` array of the Inode.
  - **Slow Symlinks**: Larger target paths allocate a standard data block. Path resolution logic correctly detects `0xA000` mode bits and redirects standard lookups.
- **Kernel-side Formatting (`mkfs.ext2`)**: A custom formatting routine capable of laying out the Superblock, BGDT, Root Inode, and Bitmaps on a raw, unformatted partition entirely from inside the running kernel.
- **Permissions Enforcement (Security)**: 
  - Created a simulated multi-user context (`CURRENT_UID`, `CURRENT_GID`).
  - Embedded a `check_access` helper into every read/write/delete operation to validate standard UNIX permissions (User/Group/Other mask checks).
  - Provided interactive shell commands (`su`, `whoami`, `chmod`, `chown`) to control file ownership and access dynamically.

---
**Summary**: We have successfully transformed MYNEWOS from a minimal OS capable of only reading an initial boot FAT32 disk into an OS sporting a complete Virtual File System that can format, read, write, navigate, cache, and secure standard Ext2 filesystems.
