# Phase 8.5: Ext4 Extent Support Implementation

This document details the final milestone of our Ext2/Ext4 implementation plan: adding read-only support for modern Ext4 Extent Trees.

## Why Ext4 Extents?
Older Ext2 filesystems rely on Direct, Single Indirect, Double Indirect, and Triple Indirect pointers to locate data blocks. For very large contiguous files, this results in significant metadata overhead (storing thousands of block pointers).
Ext4 introduces "Extents" which map a logical block range to a physical block range compactly (e.g., "Logical blocks 0-1000 are located at physical blocks 5000-6000"). When a file is formatted with the `EXT4_EXTENTS_FL` flag (`0x80000`), the `i_block` array of the Inode is repurposed into an Extent Tree node.

## What We Achieved
We've successfully added a parser that can traverse an Extent Tree and translate logical blocks to physical blocks on the disk natively within MYNEWOS!

### 1. Extent Structures
We added precise structures conforming to the Ext4 binary layout:
- **`Ext4ExtentHeader`**: The 12-byte header starting the tree node (`eh_magic = 0xF30A`), specifying the depth of the node and the number of entries.
- **`Ext4ExtentIdx`**: Internal index nodes that point to physical blocks containing deeper branches of the tree.
- **`Ext4Extent`**: Leaf nodes that actually map `ee_block` (logical start) and `ee_len` to `ee_start_lo` (physical start).

### 2. Extent Traversal Logic
We implemented the `get_extent_block_id` method. When `get_block_id` spots the `0x80000` flag, it reroutes the lookup to the extent parser. The parser automatically:
- Starts at the root node stored inside `inode.i_block`.
- Checks `eh_depth`. 
- If `eh_depth > 0`, it searches the index nodes for the correct logical block range, fetches the physical block for the next level from disk, and loops down.
- If `eh_depth == 0`, it iterates the leaf extents, confirms the logical block fits in the range, and dynamically calculates the exact physical block (`ee_start_lo + (logical - ee_block)`).

### 3. Read-Only Safety Guarantees
Because Extent Tree modification (allocating extents, splitting nodes, merging adjacent extents) is highly complex, we specifically marked this feature as **read-only**. Any call to `ensure_block_id` (a write attempt) on an extent-based file cleanly detects the flag and aborts, ensuring data safety while allowing robust compatibility.

**Conclusion**: MYNEWOS can now natively read both legacy indirect-block Ext2 files AND modern extent-based Ext4 files!
