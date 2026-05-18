use alloc::vec::Vec;
use alloc::string::String;
use alloc::sync::Arc;
use spin::Mutex;
use alloc::collections::BTreeMap;
use lazy_static::lazy_static;

pub mod fat32;
pub mod ext2;

/// The VFS Trait that all filesystem drivers must implement.
pub trait FileSystem: Send + Sync {
    fn read_file(&self, path: &str, uid: u16, gid: u16) -> Option<Vec<u8>>;
    fn write_file(&self, path: &str, data: &[u8], uid: u16, gid: u16) -> bool;
    fn delete_file(&self, path: &str, uid: u16, gid: u16) -> bool;
    fn rename_file(&self, old_path: &str, new_path: &str, uid: u16, gid: u16) -> bool;
    fn list_dir(&self, path: &str, uid: u16, gid: u16) -> Vec<String>;
    fn create_dir(&self, path: &str, uid: u16, gid: u16) -> bool;
    fn create_symlink(&self, path: &str, target: &str, uid: u16, gid: u16) -> bool;
    fn chmod(&self, path: &str, mode: u16, uid: u16, gid: u16) -> bool;
    fn chown(&self, path: &str, new_uid: u16, new_gid: u16, uid: u16, gid: u16) -> bool;
    fn analyze_fragmentation(&self) -> String {
        String::from("Fragmentation analysis not supported on this filesystem.")
    }
}

lazy_static! {
    /// Global registry of mounted filesystems.
    static ref MOUNTS: Mutex<BTreeMap<String, Arc<dyn FileSystem>>> = Mutex::new(BTreeMap::new());

    /// Simulated current process context
    pub static ref CURRENT_UID: Mutex<u16> = Mutex::new(0); // Default to root
    pub static ref CURRENT_GID: Mutex<u16> = Mutex::new(0);
}

// ... init stays similar but might need tweaks ...

/// Initialize the VFS and mount the boot partition.
pub fn init() {
    let mut mounts = MOUNTS.lock();
    
    // Mount root FAT32 (Still assuming Disk 0 for now)
    mounts.insert(String::from("/"), Arc::new(fat32::Fat32Fs::new()));
    crate::serial_println!("VFS: Root filesystem (FAT32) mounted at /");

    // Try to mount ext2 from Disk 1
    // 1. Try to read MBR
    if let Some(mbr) = crate::disk::mbr::MBR::read(1) {
        crate::serial_println!("VFS: MBR detected on Disk 1. Scanning partitions...");
        for (i, part) in mbr.partitions.iter().enumerate() {
            let lba_start = part.lba_start;
            let sys_id = part.sys_id;
            if sys_id == 0x83 { // Linux / Ext2
                crate::serial_println!("VFS: Found Ext2 partition at LBA {} (Part {})", lba_start, i);
                if let Some(ext2_fs) = ext2::Ext2Fs::try_new(1, lba_start) {
                    mounts.insert(String::from("/data"), Arc::new(ext2_fs));
                    crate::serial_println!("VFS: Data filesystem (ext2) mounted at /data");
                    return;
                }
            }
        }
    }

    // 2. Fallback: Try mounting whole disk (LBA 0)
    if let Some(ext2_fs) = ext2::Ext2Fs::try_new(1, 0) {
        mounts.insert(String::from("/data"), Arc::new(ext2_fs));
        crate::serial_println!("VFS: Data filesystem (ext2) mounted at /data (No MBR)");
    }
}

/// Helper to find the longest matching mount point for a given path.
fn find_mount(path: &str) -> (String, Arc<dyn FileSystem>) {
    let mounts = MOUNTS.lock();
    
    // Ensure the path starts with a / for matching
    let abs_path = if path.starts_with('/') {
        String::from(path)
    } else {
        alloc::format!("/{}", path)
    };
    
    // Find the longest key that is a prefix of abs_path
    let mut best_prefix = String::from("/");
    let mut best_fs = mounts.get("/").expect("Root FS not mounted").clone();
    
    for (prefix, fs) in mounts.iter() {
        if abs_path.starts_with(prefix) && prefix.len() > best_prefix.len() {
            best_prefix = prefix.clone();
            best_fs = fs.clone();
        }
    }
    
    (best_prefix, best_fs)
}

/// Helper to get the relative path within a filesystem.
fn get_rel_path(path: &str, mount_path: &str) -> String {
    let abs_path = if path.starts_with('/') {
        String::from(path)
    } else {
        alloc::format!("/{}", path)
    };

    let mut rel = &abs_path[mount_path.len()..];
    if rel.starts_with('/') {
        rel = &rel[1..];
    }
    if rel.is_empty() {
        String::from("/")
    } else {
        String::from(rel)
    }
}

// --- Public VFS API (Routing to appropriate mounts with current context) ---

pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.read_file(&rel, uid, gid)
}

pub fn write_file(path: &str, data: &[u8]) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.write_file(&rel, data, uid, gid)
}

pub fn delete_file(path: &str) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.delete_file(&rel, uid, gid)
}

pub fn rename_file(old_path: &str, new_path: &str) -> bool {
    let (mount_path, fs) = find_mount(old_path);
    let old_rel = get_rel_path(old_path, &mount_path);
    let new_rel = get_rel_path(new_path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.rename_file(&old_rel, &new_rel, uid, gid)
}

pub fn list_dir(path: &str) -> Vec<String> {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.list_dir(&rel, uid, gid)
}

pub fn create_dir(path: &str) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.create_dir(&rel, uid, gid)
}

pub fn create_symlink(path: &str, target: &str) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.create_symlink(&rel, target, uid, gid)
}

pub fn chmod(path: &str, mode: u16) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.chmod(&rel, mode, uid, gid)
}

pub fn chown(path: &str, new_uid: u16, new_gid: u16) -> bool {
    let (mount_path, fs) = find_mount(path);
    let rel = get_rel_path(path, &mount_path);
    let uid = *CURRENT_UID.lock();
    let gid = *CURRENT_GID.lock();
    fs.chown(&rel, new_uid, new_gid, uid, gid)
}

pub fn analyze_fragmentation(path: &str) -> String {
    let (_, fs) = find_mount(path);
    fs.analyze_fragmentation()
}

pub fn list_root() -> Vec<String> {
    list_dir("/")
}
