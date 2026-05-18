use core::sync::atomic::{AtomicUsize, Ordering};

/// Number of Application Processors that have come online
pub static AP_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static AP_READY_FLAG: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

// Raw bytes of the AP trampoline assembly blob
// We'll read the trampoline from the linker-provided symbols
core::arch::global_asm!(include_str!("trampoline.s"));

unsafe extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
}

/// Rust entry point for all Application Processors.
/// Called from the 64-bit portion of the SMP trampoline.
#[no_mangle]
pub extern "C" fn ap_entry() -> ! {
    let ap_id = AP_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    crate::serial_println!("SMP: Application Processor {} is ONLINE!", ap_id);

    // Load the private GDT/TSS for this core
    crate::gdt::init();

    // Load the shared IDT (interrupts are needed for scheduling)
    crate::interrupts::init_idt();

    // Initialize SYSCALL/SYSRET for this core
    crate::syscall::init();

    // Signal BSP that this core is ready
    AP_READY_FLAG.store(true, Ordering::SeqCst);

    // Enable Local APIC timer on this AP
    crate::apic::end_of_interrupt(); // Clear any pending interrupt

    x86_64::instructions::interrupts::enable();

    // Each AP runs the scheduler loop
    loop {
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

const TRAMPOLINE_PHYS_ADDR: usize = 0x8000;
const TRAMPOLINE_CR3_PTR:    usize = 0x7FF8;
const TRAMPOLINE_IDT_PTR:    usize = 0x7FE8;
const TRAMPOLINE_ENTRY_PTR:  usize = 0x7FD8;
const TRAMPOLINE_STACK_PTR:  usize = 0x7FC8;

/// Wake all Application Processors by sending INIT + SIPI via LAPIC ICR.
pub fn start_all_aps() {
    crate::serial_println!("SMP: Starting Application Processors...");

    // Copy the trampoline blob into low memory
    unsafe {
        let src = core::ptr::addr_of!(ap_trampoline_start);
        let end = core::ptr::addr_of!(ap_trampoline_end);
        let len = end as usize - src as usize;
        let dst = TRAMPOLINE_PHYS_ADDR as *mut u8;
        core::ptr::copy_nonoverlapping(src, dst, len);
        crate::serial_println!("SMP: Trampoline copied ({} bytes) to {:#X}", len, TRAMPOLINE_PHYS_ADDR);
    }

    // Write the BSP's CR3 (page table) so APs use the same mapping
    unsafe {
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3);
        let cr3_ptr = TRAMPOLINE_CR3_PTR as *mut u64;
        cr3_ptr.write_volatile(cr3);
        crate::serial_println!("SMP: Wrote CR3 ({:#X}) to trampoline data.", cr3);
    }

    // Write the ap_entry function pointer
    unsafe {
        let entry_ptr = TRAMPOLINE_ENTRY_PTR as *mut u64;
        entry_ptr.write_volatile(ap_entry as u64);
    }

    // Write the IDT pointer (IDTR register contents: 10 bytes) to trampoline
    unsafe {
        let idtr_ptr = TRAMPOLINE_IDT_PTR as *mut [u8; 10];
        core::arch::asm!("sidt [{}]", in(reg) idtr_ptr);
    }

    // Read LAPIC base from our APIC module
    let lapic_base = crate::apic::LAPIC_BASE.load(Ordering::Relaxed);
    if lapic_base == 0 {
        crate::serial_println!("SMP: LAPIC not initialized, cannot wake APs!");
        return;
    }

    // Find all AP LAPIC IDs from ACPI and send INIT + SIPI to each
    // For simplicity, we probe the LAPIC ID of the BSP, then wake cores 1..N
    // QEMU typically exposes 1 BSP (ID 0) and N-1 APs
    // We'll read the number of detected CPUs from ACPI during apic::init
    let ap_count = crate::apic::DETECTED_CPU_COUNT.load(Ordering::Relaxed);
    crate::serial_println!("SMP: Detected {} CPU(s) total.", ap_count);

    for ap_lapic_id in 1..ap_count {
        AP_READY_FLAG.store(false, Ordering::SeqCst);
        
        // Allocate a fresh 64KB kernel stack for this AP
        let stack = alloc::vec![0u8; 64 * 1024].into_boxed_slice();
        let stack_ptr = stack.as_ptr() as u64 + stack.len() as u64;
        core::mem::forget(stack); // Keep the stack alive forever
        
        // Write the stack pointer to the trampoline area
        unsafe {
            let ptr = TRAMPOLINE_STACK_PTR as *mut u64;
            ptr.write_volatile(stack_ptr);
        }

        wake_ap(lapic_base, ap_lapic_id as u8);
        // Wait for AP to finish gdt::init()
        while !AP_READY_FLAG.load(Ordering::SeqCst) {
            core::hint::spin_loop();
        }
    }

    crate::serial_println!("SMP: INIT/SIPI sent to {} APs.", ap_count.saturating_sub(1));
}

/// Sends INIT + SIPI to a single AP via the LAPIC ICR registers.
fn wake_ap(lapic_base: usize, lapic_id: u8) {
    let icr_lo = lapic_base + 0x300;
    let icr_hi = lapic_base + 0x310;

    unsafe {
        let icr_hi_ptr = icr_hi as *mut u32;
        let icr_lo_ptr = icr_lo as *mut u32;

        // Target specific LAPIC ID in ICR High
        icr_hi_ptr.write_volatile((lapic_id as u32) << 24);

        // Send INIT IPI (delivery mode = 0b101, level = 1, trigger = 0)
        icr_lo_ptr.write_volatile(0x0000_4500);
        delay_10ms();

        // Send SIPI - vector = page where trampoline is (0x8000 >> 12 = 0x08)
        let sipi_vector = (TRAMPOLINE_PHYS_ADDR >> 12) as u32;
        icr_lo_ptr.write_volatile(0x0000_4600 | sipi_vector);
        delay_1ms();

        // Send second SIPI (Intel spec recommends sending twice)
        icr_lo_ptr.write_volatile(0x0000_4600 | sipi_vector);
        delay_10ms();
    }

    crate::serial_println!("SMP: SIPI sent to LAPIC {}.", lapic_id);
}

fn delay_10ms() {
    for _ in 0..1_000_000 { core::hint::spin_loop(); }
}

fn delay_1ms() {
    for _ in 0..100_000 { core::hint::spin_loop(); }
}
