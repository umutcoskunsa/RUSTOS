use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};
use lazy_static::lazy_static;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 1;
pub const PAGE_FAULT_IST_INDEX: u16 = 2;
pub const GPF_IST_INDEX: u16 = 3;

/// Per-CPU scratch area stored in GS_BASE.
/// This allows the syscall handler to find the kernel stack without a stack to start with.
#[repr(C)]
pub struct PerCpu {
    pub kernel_stack: u64, // Offset 0
    pub user_rsp:     u64, // Offset 8
    pub tss_ptr:      u64, // Offset 16: pointer to this CPU's TSS
    pub current_pid:  usize, // Offset 24
    pub shell_rsp:    u64, // Offset 32
}

pub const MSR_GS_BASE: u32 = 0xC0000101;
pub const MSR_KERNEL_GS_BASE: u32 = 0xC0000102;


// We no longer use a global SYSCALL_KERNEL_STACK. 
// It is now allocated per-CPU during gdt::init().

/// Updates the TSS Ring 0 stack pointer for the CURRENT CPU.
pub fn set_tss_rsp0(rsp: u64) {
    unsafe {
        // In kernel context (including scheduler), GS_BASE always points to PerCpu.
        let gs_base = x86_64::registers::model_specific::Msr::new(MSR_GS_BASE).read();
        let per_cpu = &*(gs_base as *const PerCpu);
        let tss = &mut *(per_cpu.tss_ptr as *mut TaskStateSegment);
        tss.privilege_stack_table[0] = VirtAddr::new(rsp);
    }
}


pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
}

#[repr(C, align(16))]
struct AlignedTss(TaskStateSegment);

#[repr(C, align(16))]
struct AlignedGdt(GlobalDescriptorTable);

pub fn init() {
    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS, DS, ES, FS, GS, SS, Segment};
    use crate::serial_println;

    unsafe {
        let cpu_id = crate::apic::local_id();
        
        // 1. Allocate a fresh TSS for this CPU (Guaranteed 16-byte aligned)
        let mut tss_box = alloc::boxed::Box::new(AlignedTss(TaskStateSegment::new()));
        let tss = &mut tss_box.0;
        
        // Allocate Per-CPU Double Fault Stack (IST 1)
        let df_stack = alloc::vec![0u8; 4096 * 5].into_boxed_slice();
        let df_stack_ptr = df_stack.as_ptr() as u64 + df_stack.len() as u64;
        core::mem::forget(df_stack); // Keep alive forever
        tss.interrupt_stack_table[(DOUBLE_FAULT_IST_INDEX - 1) as usize] = VirtAddr::new(df_stack_ptr);

        // Page Fault Stack (IST 2)
        let pf_stack = alloc::vec![0u8; 4096 * 5].into_boxed_slice();
        let pf_stack_ptr = pf_stack.as_ptr() as u64 + pf_stack.len() as u64;
        core::mem::forget(pf_stack);
        tss.interrupt_stack_table[(PAGE_FAULT_IST_INDEX - 1) as usize] = VirtAddr::new(pf_stack_ptr);

        // GPF Stack (IST 3)
        let gpf_stack = alloc::vec![0u8; 4096 * 5].into_boxed_slice();
        let gpf_stack_ptr = gpf_stack.as_ptr() as u64 + gpf_stack.len() as u64;
        core::mem::forget(gpf_stack);
        tss.interrupt_stack_table[(GPF_IST_INDEX - 1) as usize] = VirtAddr::new(gpf_stack_ptr);

        // Allocate Per-CPU default Sycall Kernel Stack (fallback stack)
        let fallback_stack = alloc::vec![0u8; 4096 * 4].into_boxed_slice();
        let fallback_stack_ptr = fallback_stack.as_ptr() as u64 + fallback_stack.len() as u64;
        core::mem::forget(fallback_stack); // Keep alive forever
        tss.privilege_stack_table[0] = VirtAddr::new(fallback_stack_ptr);

        let tss_ref = &mut alloc::boxed::Box::leak(tss_box).0;
        let tss_ptr = tss_ref as *const _ as u64;
        
        // Safety check: The hardware MUST have a 16-byte aligned TSS or it will GPF
        if tss_ptr % 16 != 0 {
            serial_println!("CPU-GDT [CPU {}]: TSS pointer is not 16-byte aligned! ptr={:#x}", cpu_id, tss_ptr);
            panic!("TSS ALIGNMENT ERROR");
        }

        // 2. Allocate a fresh GDT for this CPU (16-byte aligned)
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector      = gdt.add_entry(Descriptor::kernel_code_segment());
        let data_selector      = gdt.add_entry(Descriptor::kernel_data_segment());
        let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
        let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());
        let tss_selector       = gdt.add_entry(Descriptor::tss_segment(tss_ref));
        
        let gdt_box = alloc::boxed::Box::new(AlignedGdt(gdt));
        let gdt_ref = &alloc::boxed::Box::leak(gdt_box).0;
        gdt_ref.load();

        serial_println!("CPU-GDT [CPU {}]: GDT loaded at {:#x}, TSS selector={:#x}", 
            cpu_id, gdt_ref as *const _ as u64, tss_selector.0);

        // 3. Reload segments
        CS::set_reg(code_selector);
        DS::set_reg(data_selector);
        ES::set_reg(data_selector);
        FS::set_reg(data_selector);
        GS::set_reg(data_selector); // WARNING: This zeros MSR_GS_BASE!
        SS::set_reg(data_selector);
        load_tss(tss_selector);

        // 4. Initialize per-CPU GS_BASE scratch area
        let per_cpu = alloc::boxed::Box::leak(alloc::boxed::Box::new(PerCpu {
            kernel_stack: fallback_stack_ptr, 
            user_rsp: 0,
            tss_ptr,
            current_pid: 0,
            shell_rsp: 0,
        }));
        let addr = per_cpu as *const _ as u64;
        
        x86_64::registers::model_specific::Msr::new(MSR_GS_BASE).write(addr);
        x86_64::registers::model_specific::Msr::new(MSR_KERNEL_GS_BASE).write(addr);
        
        serial_println!("GDT: CPU core initialized with private GDT/TSS.");
    }
}

/// Helper to get a mutable reference to the current CPU's PerCpu struct.
pub fn get_per_cpu() -> &'static mut PerCpu {
    unsafe {
        // In kernel context, GS_BASE always holds our PerCpu pointer.
        // Even after swapgs in the ASM handler, GS_BASE has the kernel value
        // (swapped from KERNEL_GS_BASE).
        let gs_base = x86_64::registers::model_specific::Msr::new(MSR_GS_BASE).read();
        &mut *(gs_base as *mut PerCpu)
    }
}
/// Returns the User Code selector (Index 4, RPL 3).
pub fn get_user_code_selector() -> SegmentSelector {
    SegmentSelector::new(4, x86_64::PrivilegeLevel::Ring3)
}

/// Returns the User Data selector (Index 3, RPL 3).
pub fn get_user_data_selector() -> SegmentSelector {
    SegmentSelector::new(3, x86_64::PrivilegeLevel::Ring3)
}
