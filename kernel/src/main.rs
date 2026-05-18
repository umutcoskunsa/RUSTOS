#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(naked_functions)]

pub mod vga_buffer;
pub mod serial;
pub mod interrupts;
pub mod gdt;
pub mod memory;
pub mod allocator;
pub mod task;
pub mod apic;
pub mod smp;
pub mod disk;
pub mod fs;
pub mod userspace;
pub mod syscall;
pub mod shell;
pub mod cap;
pub mod elf;
pub mod process;
pub mod ipc;
pub mod pci;
pub mod net;
pub mod graphics;
extern crate alloc;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    serial_println!("{}", info);
    loop {}
}

unsafe extern "C" {
    static _bss_start: u8;
    static _bss_end: u8;
}

/// Kernel entry point
#[no_mangle]
#[link_section = ".text.start"]
pub extern "C" fn kernel_main(mmap_ptr: u64, mmap_cnt: u64) -> ! {
    // Zero out .bss section early to ensure global statics (such as VGA and Serial locks) start cleared
    unsafe {
        let bss_start = core::ptr::addr_of!(_bss_start) as *mut u8;
        let bss_end = core::ptr::addr_of!(_bss_end) as *mut u8;
        let bss_size = bss_end as usize - bss_start as usize;
        core::ptr::write_bytes(bss_start, 0, bss_size);
    }

    println!("Hello World from Rust using our new VGA Buffer Driver! {}", 42);
    serial_println!("Kernel Boot successful!");

    serial_println!("Memory Map Pointer: {:#x}", mmap_ptr);
    serial_println!("Memory Map Count: {}", mmap_cnt);

    let memory_map = unsafe {
        core::slice::from_raw_parts(mmap_ptr as *const memory::MemoryMapEntry, mmap_cnt as usize)
    };

    serial_println!("--- Memory Map ---");
    for (i, entry) in memory_map.iter().enumerate() {
        serial_println!("Entry {}: Base={:#018x}, Len={:#018x}, Type={}", 
            i, entry.base_addr, entry.len, entry.entry_type);
    }

    let mut frame_allocator = unsafe { memory::BootFrameAllocator::init(memory_map) };
    
    use x86_64::structures::paging::FrameAllocator;
    if let Some(frame) = frame_allocator.allocate_frame() {
        serial_println!("Allocated first frame at: {:?}", frame.start_address());
    } else {
        serial_println!("Failed to allocate frame!");
    }

    let level_4_table = unsafe { memory::active_level_4_table() };
    serial_println!("--- Level 4 Page Table ---");
    for (i, entry) in level_4_table.iter().enumerate() {
        if !entry.is_unused() {
            serial_println!("Entry {}: {:?}", i, entry);
        }
    }

    use x86_64::structures::paging::{RecursivePageTable, Mapper};

    let mut recursive_page_table = unsafe {
        RecursivePageTable::new(level_4_table).unwrap()
    };

    let heap_start: u64 = 0x4444_4444_0000;
    let heap_size: u64 = 64 * 1024 * 1024; // 64MB - large enough for WAD file reads
    let heap_end = heap_start + heap_size;

    serial_println!("Mapping Heap pages...");
    for page_addr in (heap_start..heap_end).step_by(4096) {
        let page = x86_64::structures::paging::Page::containing_address(
            x86_64::VirtAddr::new(page_addr)
        );
        let frame = frame_allocator.allocate_frame().unwrap();
        
        let flags = x86_64::structures::paging::PageTableFlags::PRESENT | 
                    x86_64::structures::paging::PageTableFlags::WRITABLE;
                    
        unsafe {
            recursive_page_table.map_to(page, frame, flags, &mut frame_allocator)
                .unwrap()
                .flush();
        }
    }
    serial_println!("Heap mapped successfully!");

    unsafe {
        allocator::ALLOCATOR.lock().init(heap_start as usize, heap_size as usize);
    }

    // Save mapper and allocator to global state for APIC memory mapping
    *memory::GLOBAL_MAPPER.lock() = Some(recursive_page_table);
    *memory::GLOBAL_FRAME_ALLOCATOR.lock() = Some(frame_allocator);

    use alloc::vec::Vec;
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    serial_println!("Allocated Vec successfully: {:?}", v);

    serial_println!("Testing continuous heap allocations for Linked List Allocator...");
    for i in 0..10_000 {
        let x = alloc::boxed::Box::new(i);
        if *x != i {
            panic!("Heap Allocation Error at index {}", i);
        }
    }
    serial_println!("Passed 10,000 heap allocations without Out-Of-Memory!");

    interrupts::init_idt();
    gdt::init();

    // Mask legacy PIC so it does not interfere with APIC
    unsafe { interrupts::PICS.lock().initialize() };

    // Init APIC & Parse ACPI
    apic::init();

    // Wake up Application Processors
    smp::start_all_aps();

    // Detect and initialize the ATA disk
    if disk::detect(0) {
        crate::serial_println!("DISK: ATA Master disk detected.");
        // Initialize the VFS and mount the root filesystem
        crate::fs::init();
    } else {
        crate::serial_println!("DISK: No ATA disk found (no -hda?)");
    }

    // Initialize Graphics Engine (reads VBE info left by bootloader)
    crate::graphics::init();
    if crate::graphics::is_active() {
        let w = crate::graphics::width();
        let h = crate::graphics::height();
        crate::vga_buffer::WRITER.lock().init_graphics(w, h);
    }

    crate::pci::scan_bus();
    crate::net::init();

    // Initialize SYSCALL/SYSRET mechanism
    syscall::init();

    // Enable SSE and FPU support
    unsafe {
        use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};
        let mut cr0 = Cr0::read();
        cr0.remove(Cr0Flags::EMULATE_COPROCESSOR);
        cr0.insert(Cr0Flags::MONITOR_COPROCESSOR);
        Cr0::write(cr0);

        let mut cr4 = Cr4::read();
        cr4.insert(Cr4Flags::OSFXSR);
        cr4.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        Cr4::write(cr4);
    }

    println!("It did not crash! Resumed execution!");
    println!("Starting Shell...");

    // Initialize the async keyboard task before the shell uses it
    task::keyboard::SCANCODE_QUEUE.try_init_once(|| crossbeam_queue::ArrayQueue::new(100)).ok();

    x86_64::instructions::interrupts::enable();

    // Run the interactive shell (blocks forever in a prompt loop)
    shell::run();
}
