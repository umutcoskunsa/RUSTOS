/// Process management: load ELF binaries, track process state, run in Ring 3.
///
/// For now processes share the kernel page table (full isolation comes later).
/// This is safe because user code runs at Ring 3 and cannot access kernel pages
/// (they are mapped without the USER_ACCESSIBLE flag).
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use lazy_static::lazy_static;
use x86_64::structures::paging::FrameAllocator;
use crate::gdt;

/// Next PID to assign (starts at 1; 0 = kernel)
static NEXT_PID: AtomicUsize = AtomicUsize::new(1);

// We no longer use a global CURRENT_PID. It is now stored in PerCpu (gdt.rs).

/// Size of the per-process user stack
const USER_STACK_SIZE: usize = 64 * 1024; // 64 KiB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Zombie(i64),
}

#[repr(C, packed)]
pub struct InterruptFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9:  u64, pub r8:  u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub rip: u64, pub cs:  u64, pub rflags: u64, pub rsp:  u64, pub ss:  u64,
}

pub struct Process {
    pub pid:          usize,
    pub name:         alloc::string::String,
    pub state:        ProcessState,
    pub cr3:          u64,                     // Physical address of this process's PML4
    pub kernel_stack: alloc::boxed::Box<[u8]>, // Ring 0 stack allocation
    pub kernel_rsp:   u64,                     // RSP referencing the InterruptFrame
}

// We no longer use a global KERNEL_THREAD_RSP. It is now stored in PerCpu (gdt.rs).

/// Global process table
lazy_static! {
    pub static ref PROCESS_TABLE: Mutex<Vec<Process>> = Mutex::new(Vec::new());
}

/// Spawn a new process from an ELF binary.
/// Creates an fully isolated page table with its own PML4, mapping the kernel
/// in the bottom half and the user ELF + stack in the upper half.
pub fn spawn(elf_bytes: &[u8], name: &str) -> Result<usize, &'static str> {
    if !crate::elf::is_elf(elf_bytes) {
        return Err("Not an ELF binary");
    }

    x86_64::instructions::interrupts::without_interrupts(|| {
        // 1. Allocate a new physical frame for the process PML4
        let mut root_frame = {
            let mut fa = crate::memory::GLOBAL_FRAME_ALLOCATOR.lock();
            fa.as_mut().unwrap().allocate_frame().ok_or("No frames for PML4")?
        };
        let new_pml4_phys = root_frame.start_address().as_u64();
        
        // 2. Ensure the new PML4 is accessible to the kernel (Identity Map it)
        crate::memory::map_identity_region(new_pml4_phys, new_pml4_phys + 4096);

        // 3. Clear the user space PML4[1] and copy all other kernel mappings
        //    from the current active PML4 (identity map, heap, recursive, etc.)
        let current_cr3 = read_cr3();
        let current_pml4 = unsafe { &*(current_cr3 as *const x86_64::structures::paging::PageTable) };
        
        let new_pml4 = unsafe { &mut *(new_pml4_phys as *mut x86_64::structures::paging::PageTable) };
        for i in 0..512 {
            if i == 1 {
                new_pml4[i].set_unused(); // User Space (0x80_0000_0000 to 0x100_0000_0000)
            } else if i == 511 {
                // Self-referential recursive mapping for the new isolated page table
                use x86_64::{PhysAddr, structures::paging::PageTableFlags};
                new_pml4[i].set_addr(
                    PhysAddr::new(new_pml4_phys),
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                );
            } else {
                new_pml4[i] = current_pml4[i].clone(); // Kernel memory (Heap, Identity, Recursive)
            }
        }

        // 3. Switch CR3 to the new process page table.
        // This is safe because PML4[0] is identical to the old table, and the
        // kernel executes entirely within PML4[0].
        unsafe { write_cr3(new_pml4_phys); }

        // 4. Map ELF segments and load data into the NEW isolated address space
        if let Err(e) = map_elf_segments(elf_bytes) {
            unsafe { write_cr3(current_cr3); }
            return Err(e);
        }
        let entry_point = match crate::elf::load(elf_bytes) {
            Ok(ep) => ep,
            Err(_) => {
                unsafe { write_cr3(current_cr3); }
                return Err("ELF load failed");
            }
        };

        // 5. Map the user stack in the new address space
        let stack_base = crate::userspace::USER_STACK_VIRT - USER_STACK_SIZE as u64;
        if let Err(e) = map_user_accessible(stack_base, USER_STACK_SIZE) {
            unsafe { write_cr3(current_cr3); }
            return Err(e);
        }
        let stack_top = crate::userspace::USER_STACK_VIRT - 8; // 16-byte aligned

        // 6. Restore the kernel's original CR3
        unsafe { write_cr3(current_cr3); }

        // 7. Craft the initial interrupt stack frame manually on the kernel stack
        let mut kernel_stack = alloc::vec![0u8; 64 * 1024].into_boxed_slice();
        let kernel_stack_end = kernel_stack.as_ptr() as u64 + kernel_stack.len() as u64;
        
        let frame_ptr = (kernel_stack_end - core::mem::size_of::<InterruptFrame>() as u64) as *mut InterruptFrame;
        unsafe {
            core::ptr::write(frame_ptr, InterruptFrame {
                rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
                r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
                rip: entry_point,
                cs:  gdt::get_user_code_selector().0 as u64,     // Ring 3 Code
                rflags: 0x202,     // Interrupts Enabled
                rsp: stack_top,
                ss:  gdt::get_user_data_selector().0 as u64,     // Ring 3 Data
            });
        }

        let kernel_rsp = frame_ptr as u64;

        let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
        let proc = Process {
            pid,
            name: alloc::string::String::from(name),
            state: ProcessState::Ready,
            cr3: new_pml4_phys,
            kernel_stack,
            kernel_rsp,
        };

        PROCESS_TABLE.lock().push(proc);
        crate::serial_println!(
            "PROCESS: Isolated PID {} ({}) CR3={:#x} entry={:#x} rsp={:#x}",
            pid, name, new_pml4_phys, entry_point, stack_top
        );
        Ok(pid)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn process_schedule(old_rsp: u64) -> u64 {
    // 1. Tick and acknowledge hardware timer
    crate::interrupts::TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    crate::apic::end_of_interrupt();

    let mut table = PROCESS_TABLE.lock();
    let per_cpu = crate::gdt::get_per_cpu();
    let current_pid = per_cpu.current_pid;

    // 2. Save current RSP
    if current_pid == 0 {
        // Interrupted the OS Shell thread; stash its stack pointer!
        per_cpu.shell_rsp = old_rsp;
    } else {
        if let Some(p) = table.iter_mut().find(|x| x.pid == current_pid) {
            p.kernel_rsp = old_rsp; // Save the exact interruption frame pointer
            if p.state == ProcessState::Running {
                p.state = ProcessState::Ready;
            }
        }
    }

    // 3. Find next available Ready process via Round-Robin
    let next_proc = table.iter_mut().find(|x| x.state == ProcessState::Ready);
    
    match next_proc {
        Some(p) => {
            p.state = ProcessState::Running;
            per_cpu.current_pid = p.pid;
            
            // Re-arm state: Update the TSS so future hardware interrupts use THIS kernel stack
            let kstack_top = p.kernel_stack.as_ptr() as u64 + p.kernel_stack.len() as u64;
            crate::gdt::set_tss_rsp0(kstack_top);
            // Also update PerCpu.kernel_stack for syscall stack switching
            per_cpu.kernel_stack = kstack_top;
            
            // Switch Virtual Memory Address Spaces safely!
            unsafe { write_cr3(p.cr3); }
            p.kernel_rsp // Hand physical CPU execution to this process's context!
        },
        None => {
            // No processes are active! Retreat to the Shell Thread.
            per_cpu.current_pid = 0;
            let shell_rsp = per_cpu.shell_rsp;
            
            // If shell_rsp is 0, we were already in the shell and no processes ever launched,
            // so we just return the same old_rsp!
            if shell_rsp == 0 { return old_rsp; }

            // Otherwise, we exited from processes and are jumping back to Shell.
            unsafe { write_cr3(0x70000); } // Identity Map
            shell_rsp 
        }
    }
}

/// Terminate the current process and trigger the next schedule instantly
pub fn exit(code: u64) -> ! {
    let per_cpu = crate::gdt::get_per_cpu();
    let pid = per_cpu.current_pid;
    crate::serial_println!("PROCESS: PID {} exited symmetrically with code {}", pid, code);

    {
        let mut table = PROCESS_TABLE.lock();
        if let Some(p) = table.iter_mut().find(|x| x.pid == pid) {
            p.state = ProcessState::Zombie(code as i64);
        }
    }

    // Force an early CPU context switch immediately! (Vector 32 = Timer)
    unsafe { core::arch::asm!("int 32"); }
    
    // Safety fallback
    loop { x86_64::instructions::hlt(); }
}

/// Terminate a process by PID
pub fn kill(pid: usize) -> Result<(), &'static str> {
    let mut table = PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|x| x.pid == pid) {
        if let ProcessState::Zombie(_) = p.state {
            return Err("Process is already a zombie");
        }
        p.state = ProcessState::Zombie(-1); // -1 signifies killed
        crate::serial_println!("PROCESS: PID {} ({}) was killed", pid, p.name);
        Ok(())
    } else {
        Err("Process not found")
    }
}

// ---- CR3 helpers ----

fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) cr3); }
    cr3
}

unsafe fn write_cr3(val: u64) {
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) val); }
}

// ---- Memory helpers ----

/// For each PT_LOAD segment in the ELF, allocate physical frames and map
/// the virtual address range with USER_ACCESSIBLE so Ring 3 can execute it.
fn map_elf_segments(elf_bytes: &[u8]) -> Result<(), &'static str> {
    if elf_bytes.len() < 64 { return Err("ELF too small"); }

    let hdr = unsafe { &*(elf_bytes.as_ptr() as *const crate::elf::Elf64Header) };
    let ph_offset  = hdr.e_phoff  as usize;
    let ph_entsize = hdr.e_phentsize as usize;
    let ph_count   = hdr.e_phnum  as usize;

    if ph_offset + ph_entsize * ph_count > elf_bytes.len() {
        return Err("ELF program headers out of bounds");
    }

    for i in 0..ph_count {
        let ph = unsafe {
            &*(elf_bytes[ph_offset + i * ph_entsize..].as_ptr()
                as *const crate::elf::Elf64ProgramHeader)
        };
        if ph.p_type != 1 { continue; } // PT_LOAD only

        let vaddr = ph.p_vaddr;
        let memsz = ph.p_memsz as usize;

        // Align down/up to page boundaries
        let page_start = vaddr & !0xFFF;
        let page_end   = (vaddr + memsz as u64 + 0xFFF) & !0xFFF;
        let num_pages  = ((page_end - page_start) / 4096) as usize;

        map_user_accessible(page_start, num_pages * 4096)?;
    }
    Ok(())
}

/// Re-map a kernel-heap virtual range to add the USER_ACCESSIBLE flag.
/// This is needed so Ring 3 code can execute/read/write the pages.
fn map_user_accessible(virt_start: u64, size: usize) -> Result<(), &'static str> {
    use x86_64::{
        VirtAddr,
        structures::paging::{
            Mapper, Page, PageTableFlags, Size4KiB,
            FrameAllocator, // must be in scope for allocate_frame()
        },
    };

    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE;

    let mut mapper_guard = crate::memory::GLOBAL_MAPPER.lock();
    let mut fa_guard     = crate::memory::GLOBAL_FRAME_ALLOCATOR.lock();
    let mapper = mapper_guard.as_mut().ok_or("Mapper not init")?;
    let fa     = fa_guard.as_mut().ok_or("Frame allocator not init")?;

    let num_pages = (size + 4095) / 4096;
    for p in 0..num_pages {
        let virt = VirtAddr::new(virt_start + p as u64 * 4096);
        let page: Page<Size4KiB> = Page::containing_address(virt);

        if let Ok(_phys) = mapper.translate_page(page) {
            // Page is already mapped (e.g. kernel identity map) —
            // just update the flags in-place, no unmap needed
            unsafe {
                mapper.update_flags(page, flags)
                    .map_err(|_| "update_flags failed")?.flush();
            }
        } else {
            // Not mapped — allocate a fresh physical frame and map it
            let frame = fa.allocate_frame().ok_or("Out of physical frames")?;
            unsafe {
                mapper.map_to(page, frame, flags, fa)
                    .map_err(|_| "map_to failed")?.flush();
            }
        }
    }
    Ok(())
}
