# Symbolic Links (Symlinks) in Ext2

## 1. What are Symlinks?
A Symbolic Link (symlink) is a special type of file that points to another file or directory. Unlike hard links (which point to the same inode), a symlink is a separate file that contains a **path string**.

When the operating system encounters a symlink during path resolution, it "redirects" the lookup to the path stored inside the symlink.

## 2. Ext2 Implementation Details

In Ext2, symlinks are handled in two different ways depending on the length of the target path:

### A. Fast Symlinks
If the target path is **shorter than 60 bytes**, Ext2 stores the path directly inside the inode's `i_block` array. 
- **Advantage**: No disk blocks need to be allocated, and no extra disk I/O is required to read the link destination.
- **Detection**: An inode is a symlink if its `i_mode` has the `0xA000` bit set. If the `i_size` is < 60, it is a Fast Symlink.

### B. Slow Symlinks
If the target path is **60 bytes or longer**, Ext2 allocates a data block and stores the path there, just like a regular file.
- **Detection**: Inode `i_mode` is `0xA000` and `i_size` is >= 60.

## 3. Path Resolution Logic
The VFS must be updated to handle symlinks recursively. 
1. When traversing `/data/my_link/file.txt`:
2. Resolve `data` -> Inode ID.
3. Resolve `my_link` -> Inode ID.
4. Check if `my_link` is a symlink.
5. If yes, read the target (e.g., `../other_folder`).
6. Resolve the target relative to the current directory.
7. Continue the original resolution (`file.txt`).

> [!WARNING]
> To prevent infinite loops (e.g., `link1 -> link2 -> link1`), we must implement a **Recursion Limit** (typically 8 or 16).

## 4. Implementation Steps
1. **Trait Update**: Add `create_symlink` to the `FileSystem` trait.
2. **Ext2 Support**: Implement symlink creation and reading logic.
3. **VFS Update**: Update `resolve_path` to handle redirection.
4. **Shell**: Add the `ln` command.
