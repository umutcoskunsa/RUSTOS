pub mod acpi_handler;

use acpi::{AcpiTables, InterruptModel};
use acpi_handler::IdentityAcpiHandler;
use core::sync::atomic::{AtomicUsize, Ordering};

pub static LAPIC_BASE: AtomicUsize = AtomicUsize::new(0);
pub static DETECTED_CPU_COUNT: AtomicUsize = AtomicUsize::new(0);

pub fn end_of_interrupt() {
    let base = LAPIC_BASE.load(Ordering::Relaxed);
    if base != 0 {
        let eoi_ptr = (base + 0xB0) as *mut u32;
        unsafe { eoi_ptr.write_volatile(0) };
    }
}

pub fn init() {
    crate::serial_println!("APIC: Initializing ACPI and APIC...");

    // ACPI tables are typically memory mapped. We search for the RSDP in legacy BIOS limits.
    let handler = IdentityAcpiHandler;
    let tables = unsafe {
        // Search the legacy BIOS area (0xE0000 - 0xFFFFF) for the RSDP pointer
        AcpiTables::search_for_rsdp_bios(handler.clone()).expect("Failed to find RSDP in legacy BIOS region!")
    };

    crate::serial_println!("APIC: Successfully parsed ACPI tables!");

    if let Ok(platform_info) = acpi::PlatformInfo::new(&tables) {
        if let InterruptModel::Apic(apic_info) = platform_info.interrupt_model {
            crate::serial_println!("APIC: Found APIC Interrupt Model!");
            crate::serial_println!("APIC: Local APIC base address: {:#X}", apic_info.local_apic_address);
            LAPIC_BASE.store(apic_info.local_apic_address as usize, Ordering::Relaxed);
            crate::serial_println!("APIC: IO APICs count: {}", apic_info.io_apics.len());

            // Count all processors via PlatformInfo
            let cpu_count = platform_info.processor_info
                .as_ref()
                .map(|p| p.application_processors.len() + 1) // +1 for BSP
                .unwrap_or(1);
            DETECTED_CPU_COUNT.store(cpu_count, Ordering::Relaxed);
            crate::serial_println!("APIC: Total CPU count: {}", cpu_count);
            
            unsafe {
                // Completely mask legacy 8259 PIC
                crate::interrupts::PICS.lock().write_masks(0xFF, 0xFF);
                crate::serial_println!("APIC: Legacy 8259 PIC masked.");

                // Identity map LAPIC
                crate::memory::map_identity_region(apic_info.local_apic_address as u64, (apic_info.local_apic_address as u64) + 0x1000);
                crate::serial_println!("APIC: Local APIC base mapped.");

                // Enable Local APIC by setting SVR (Spurious Vector Register) bit 8
                let svr_ptr = (apic_info.local_apic_address as u64 + 0xF0) as *mut u32;
                svr_ptr.write_volatile(0xFF | (1 << 8));
                crate::serial_println!("APIC: Local APIC enabled!");

                // Configure APIC Timer
                // Divide Configuration Register (0x3E0) - Divide by 16
                let timer_div_ptr = (apic_info.local_apic_address as u64 + 0x3E0) as *mut u32;
                timer_div_ptr.write_volatile(0x3);

                // LVT Timer Register (0x320) - Periodic mode (Bit 17) + IDT Vector (Timer is Vector 32)
                let timer_lvt_ptr = (apic_info.local_apic_address as u64 + 0x320) as *mut u32;
                timer_lvt_ptr.write_volatile(32 | (1 << 17));

                // Initial Count Register (0x380)
                let timer_init_ptr = (apic_info.local_apic_address as u64 + 0x380) as *mut u32;
                timer_init_ptr.write_volatile(0x100000); // Arbitrary tick value

                crate::serial_println!("APIC: Local APIC Timer configured!");
                
                if !apic_info.io_apics.is_empty() {
                    let ioapic_addr = apic_info.io_apics[0].address as u64;
                    crate::memory::map_identity_region(ioapic_addr, ioapic_addr + 0x1000);

                    let ioregsel = ioapic_addr as *mut u32;
                    let iowin = (ioapic_addr + 0x10) as *mut u32;

                    // Default Keyboard IRQ 1 mapping to IOAPIC Pin 1
                    let pin = 1;
                    let vector = 33; // InterruptIndex::Keyboard

                    // Write lower 32 bits (Vector)
                    ioregsel.write_volatile(0x10 + (pin * 2));
                    iowin.write_volatile(vector);

                    // Write upper 32 bits (Destination LAPIC = 0)
                    ioregsel.write_volatile(0x10 + (pin * 2) + 1);
                    iowin.write_volatile(0);

                    crate::serial_println!("APIC: IOAPIC configured for Keyboard (IRQ 1 -> Vector 33).");
                }
            }
        } else {
            panic!("APIC is not supported by this hardware!");
        }
    }
}
