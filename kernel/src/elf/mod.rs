/// ELF64 binary loader — parses standard ELF executables and loads them into memory.
/// Supports: ET_EXEC (static executable), PT_LOAD segments, x86_64 only.
/// Does NOT support: dynamic linking (DT_INTERP), relocations, or 32-bit ELF.

// User programs load at the Linux-compatible base address
pub const USER_LOAD_BASE: u64 = 0x40_0000;

// ELF magic and constants
const ELF_MAGIC:   [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64:  u8  = 2;
const ELFDATA2LSB: u8  = 1;   // Little-endian
const EM_X86_64:   u16 = 62;  // 0x3E
const PT_LOAD:     u32 = 1;

/// ELF64 File Header (64 bytes)
#[repr(C)]
pub struct Elf64Header {
    pub e_ident:      [u8; 16],
    pub e_type:       u16,
    pub e_machine:    u16,
    pub e_version:    u32,
    pub e_entry:      u64,
    pub e_phoff:      u64,
    pub e_shoff:      u64,
    pub e_flags:      u32,
    pub e_ehsize:     u16,
    pub e_phentsize:  u16,
    pub e_phnum:      u16,
    pub e_shentsize:  u16,
    pub e_shnum:      u16,
    pub e_shstrndx:   u16,
}

// Public aliases used by process::mod
pub use Elf64Header as Elf64HeaderPub;

/// ELF64 Program Header (56 bytes)
#[repr(C)]
pub struct Elf64ProgramHeader {
    pub p_type:   u32,
    pub p_flags:  u32,
    pub p_offset: u64,
    pub p_vaddr:  u64,
    pub p_paddr:  u64,
    pub p_filesz: u64,
    pub p_memsz:  u64,
    pub p_align:  u64,
}

pub use Elf64ProgramHeader as Elf64ProgramHeaderPub;

/// Possible errors from ELF loading
#[derive(Debug)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    Not64Bit,
    NotLittleEndian,
    NotX86_64,
    DynamicNotSupported,
    BadProgramHeader,
    SegmentOutOfBounds,
}

/// Validate and parse an ELF64 binary.
/// Returns the virtual entry point address on success.
///
/// The bytes are loaded directly into their virtual addresses; the caller
/// must ensure those pages are writable (they will be mapped by the process
/// page table setup before this is called).
pub fn load(bytes: &[u8]) -> Result<u64, ElfError> {
    if bytes.len() < core::mem::size_of::<Elf64Header>() {
        return Err(ElfError::TooSmall);
    }

    // Safety: we just checked the buffer is large enough
    let hdr = unsafe { &*(bytes.as_ptr() as *const Elf64Header) };

    // Validate magic
    if hdr.e_ident[..4] != ELF_MAGIC { return Err(ElfError::BadMagic); }
    if hdr.e_ident[4] != ELFCLASS64  { return Err(ElfError::Not64Bit); }
    if hdr.e_ident[5] != ELFDATA2LSB { return Err(ElfError::NotLittleEndian); }
    if hdr.e_machine != EM_X86_64    { return Err(ElfError::NotX86_64); }

    // Reject dynamic executables (require an interpreter / dynamic linker)
    // Type 3 = ET_DYN, but PIE executables also use ET_DYN — we accept both
    // as long as there is no PT_INTERP segment (checked below)

    let ph_offset  = hdr.e_phoff as usize;
    let ph_entsize = hdr.e_phentsize as usize;
    let ph_count   = hdr.e_phnum as usize;

    if ph_entsize < core::mem::size_of::<Elf64ProgramHeader>() {
        return Err(ElfError::BadProgramHeader);
    }
    if ph_offset + ph_entsize * ph_count > bytes.len() {
        return Err(ElfError::BadProgramHeader);
    }

    // First pass: reject dynamic (PT_INTERP) segments
    for i in 0..ph_count {
        let ph = ph_at(bytes, ph_offset, ph_entsize, i);
        if ph.p_type == 3 { // PT_INTERP
            return Err(ElfError::DynamicNotSupported);
        }
    }

    // Second pass: load PT_LOAD segments
    for i in 0..ph_count {
        let ph = ph_at(bytes, ph_offset, ph_entsize, i);
        if ph.p_type != PT_LOAD { continue; }

        let file_off = ph.p_offset as usize;
        let filesz   = ph.p_filesz as usize;
        let memsz    = ph.p_memsz  as usize;
        let vaddr    = ph.p_vaddr  as usize;

        if file_off + filesz > bytes.len() {
            return Err(ElfError::SegmentOutOfBounds);
        }

        // Write file bytes to virtual address
        // Safety: vaddr is a user-space virtual address that the caller has
        // mapped as writable before calling load().
        unsafe {
            let dst = vaddr as *mut u8;
            core::ptr::copy_nonoverlapping(bytes[file_off..].as_ptr(), dst, filesz);
            // Zero-fill BSS (.bss section is memsz - filesz bytes of zeros)
            if memsz > filesz {
                core::ptr::write_bytes(dst.add(filesz), 0, memsz - filesz);
            }
        }

        crate::serial_println!(
            "ELF: Loaded segment {}: vaddr={:#x} filesz={} memsz={}",
            i, vaddr, filesz, memsz
        );
    }

    crate::serial_println!("ELF: Entry point = {:#x}", hdr.e_entry);
    Ok(hdr.e_entry)
}

/// Helper: get a reference to the i-th program header
fn ph_at(bytes: &[u8], ph_offset: usize, ph_entsize: usize, i: usize) -> &Elf64ProgramHeader {
    let off = ph_offset + i * ph_entsize;
    unsafe { &*(bytes[off..].as_ptr() as *const Elf64ProgramHeader) }
}

/// Check if a byte slice starts with the ELF magic bytes.
/// Used by the shell to detect ELF vs flat binary without full parsing.
pub fn is_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == ELF_MAGIC
}
