use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB, RecursivePageTable},
    PhysAddr,
};
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref GLOBAL_MAPPER: Mutex<Option<RecursivePageTable<'static>>> = Mutex::new(None);
    pub static ref GLOBAL_FRAME_ALLOCATOR: Mutex<Option<BootFrameAllocator>> = Mutex::new(None);
}

/// A single entry in the system memory map (E820 format).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryMapEntry {
    pub base_addr: u64,
    pub len: u64,
    pub entry_type: u32,
    pub acpi_extended: u32,
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootFrameAllocator {
    memory_map: &'static [MemoryMapEntry],
    current_idx: usize,
    current_frame: u64,
}

impl BootFrameAllocator {
    /// Create a FrameAllocator from a passed memory map.
    ///
    /// # Safety
    /// The caller must guarantee that the memory map is valid and accurate.
    pub unsafe fn init(memory_map: &'static [MemoryMapEntry]) -> Self {
        let mut allocator = BootFrameAllocator {
            memory_map,
            current_idx: 0,
            current_frame: 0,
        };
        allocator.advance_to_next_usable();
        allocator
    }

    /// Advances the internal cursors to the next valid frame space, skipping non-usable parts.
    fn advance_to_next_usable(&mut self) {
        const MIN_ALLOC_ADDR: u64 = 0x200000; // 2MB: Skip bootloader, stack, and kernel binary

        while self.current_idx < self.memory_map.len() {
            let entry = &self.memory_map[self.current_idx];
            if entry.entry_type == 1 {
                if self.current_frame == 0 {
                    self.current_frame = core::cmp::max(entry.base_addr, MIN_ALLOC_ADDR);
                }
                if self.current_frame < (entry.base_addr + entry.len) {
                    break;
                }
            }
            self.current_idx += 1;
            self.current_frame = 0;
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        if self.current_idx >= self.memory_map.len() {
            return None;
        }

        let frame = PhysFrame::containing_address(PhysAddr::new(self.current_frame));

        // Advance cursor address
        self.current_frame += 4096;

        // Verify bounds on current entry
        let entry = &self.memory_map[self.current_idx];
        if self.current_frame >= (entry.base_addr + entry.len) {
            self.current_idx += 1;
            self.current_frame = 0;
            self.advance_to_next_usable();
        }

        Some(frame)
    }
}

use x86_64::structures::paging::PageTable;

/// Returns a mutable reference to the active level 4 page table.
///
/// # Safety
/// The caller must guarantee that the level 4 table is recursively mapped to index 511.
pub unsafe fn active_level_4_table() -> &'static mut PageTable {
    let ptr = 0xFFFF_FFFF_FFFF_F000 as *mut PageTable;
    unsafe { &mut *ptr }
}

/// Dynamically identity maps a contiguous physical region. Used primarily by ACPI/APIC.
pub fn map_identity_region(start_phys: u64, end_phys: u64) {
    use x86_64::structures::paging::{Page, PhysFrame, Mapper, PageTableFlags};
    use x86_64::{VirtAddr, PhysAddr};

    let mut mapper_guard = GLOBAL_MAPPER.lock();
    let mut frame_alloc_guard = GLOBAL_FRAME_ALLOCATOR.lock();

    if let (Some(mapper), Some(frame_alloc)) = (mapper_guard.as_mut(), frame_alloc_guard.as_mut()) {
        for addr in (start_phys..end_phys).step_by(4096) {
            let page: Page<x86_64::structures::paging::Size4KiB> = Page::containing_address(VirtAddr::new(addr));
            let frame = PhysFrame::containing_address(PhysAddr::new(addr));
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;
            
            // map_to might return an error if already mapped, which is fine to ignore for identity
            unsafe {
                if let Ok(mapper_result) = mapper.map_to(page, frame, flags, frame_alloc) {
                    mapper_result.flush();
                }
            }
        }
    } else {
        crate::serial_println!("CRITICAL ERROR: Unable to identity map {:#X}. Mapper not initialized!", start_phys);
    }
}
