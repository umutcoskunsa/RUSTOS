/// Syscall/SYSRET setup and dispatch.
/// Configures the IA32_STAR, IA32_LSTAR, and IA32_FMASK MSRs.
use core::arch::asm;

// Syscall numbers (Linux-compatible subset)
pub const SYS_WRITE:     u64 = 1;
pub const SYS_GETPID:    u64 = 39;
pub const SYS_EXIT:      u64 = 60;
pub const SYS_KILL:      u64 = 62;
pub const SYS_READ_FILE: u64 = 300; // Custom

/// Initialize the SYSCALL/SYSRET mechanism.
pub fn init() {
    unsafe {
        // IA32_EFER: Enable SCE (System Call Extensions) bit 0
        let efer: u64;
        asm!("rdmsr", in("ecx") 0xC0000080u32, out("eax") efer, out("edx") _);
        asm!("wrmsr", in("ecx") 0xC0000080u32, in("eax") efer | 1, in("edx") 0u32);

        // IA32_STAR (MSR 0xC0000081):
        // Bits 63:48 = SYSRET CS/SS (user CS = this + 16, user SS = this + 8)
        // Bits 47:32 = SYSCALL CS/SS (kernel CS = this, kernel SS = this + 8)
        //
        // GDT layout:
        //   0x00 = null
        //   0x08 = kernel code  (Ring 0)
        //   0x10 = kernel data  (Ring 0)
        //   0x18 = user data    (Ring 3) ← STAR high = 0x18, sysret uses 0x18|3=SS and 0x20|3=CS
        //   0x20 = user code    (Ring 3)
        //   0x28 = TSS
        let star_kernel: u64 = 0x0008; // Kernel: CS=0x08, SS=0x10
        let star_user:   u64 = 0x0018; // SYSRET: SS=0x18|3, CS=0x20|3
        let star: u64 = (star_user << 48) | (star_kernel << 32);
        asm!("wrmsr", in("ecx") 0xC0000081u32, in("eax") star as u32, in("edx") (star >> 32) as u32);

        // IA32_LSTAR (MSR 0xC0000082): address of the syscall handler
        let handler = syscall_handler as u64;
        asm!("wrmsr", in("ecx") 0xC0000082u32, in("eax") handler as u32, in("edx") (handler >> 32) as u32);

        // IA32_FMASK (MSR 0xC0000084): mask applied to RFLAGS on syscall entry
        // Clear the Interrupt Flag (IF) so interrupts are disabled in the handler
        asm!("wrmsr", in("ecx") 0xC0000084u32, in("eax") 0x200u32, in("edx") 0u32);
    }

    crate::serial_println!("SYSCALL: SYSCALL/SYSRET mechanism initialized.");
}

/// Raw syscall entry point — naked function to preserve all registers exactly.
#[unsafe(naked)]
unsafe extern "C" fn syscall_handler() {
    // On SYSCALL entry:
    //   RAX = syscall number
    //   RDI = arg1, RSI = arg2, RDX = arg3
    //   RCX = return RIP (saved by hardware), R11 = saved RFLAGS
    // We must save/restore everything and call into Rust dispatch.
    core::arch::naked_asm!(
        // 1. Switch to Kernel Stack
        "swapgs",                         // Swap GS with KERNEL_GS_BASE (contains PerCpu ptr)
        "mov gs:[8], rsp",                // Save user RSP to PerCpu.user_rsp (offset 8)
        "mov rsp, gs:[0]",                // Load kernel RSP from PerCpu.kernel_stack (offset 0)
        
        // 2. Save caller-saved registers (on the new KERNEL stack)
        "push rcx",                       // Hardware saved RIP
        "push r11",                       // Hardware saved RFLAGS
        "push rbp",
        "push rdi", "push rsi", "push rdx",

        // 3. Dispatch to Rust
        "mov rcx, rax",                   // 4th arg = syscall number
        "call {dispatch}",
        
        // 4. Restore registers
        "pop rdx", "pop rsi", "pop rdi",
        "pop rbp",
        "pop r11",
        "pop rcx",

        // 5. Switch back to User Stack
        "mov gs:[0], rsp",                // Save current kernel stack top back for next entry
        "mov rsp, gs:[8]",                // Restore user RSP
        "swapgs",                         // Restore user GS base
        "sysretq",
        dispatch = sym dispatch,
    );
}

/// Rust syscall dispatcher — called from the naked handler above.
extern "C" fn dispatch(arg1: u64, arg2: u64, arg3: u64, number: u64) -> u64 {
    match number {
        SYS_WRITE  => sys_write(arg1, arg2 as *const u8, arg3),
        SYS_GETPID => sys_getpid(),
        SYS_EXIT   => sys_exit(arg1),
        SYS_KILL   => sys_kill(arg1),
        _          => u64::MAX, // ENOSYS
    }
}

/// SYS_WRITE: write `len` bytes from `buf_ptr` to fd (we only support stdout=1)
fn sys_write(fd: u64, buf_ptr: *const u8, len: u64) -> u64 {
    if fd != 1 { return u64::MAX; }
    // Safety: buffer pointer comes from user space — in a real OS we'd validate this
    let bytes = unsafe { core::slice::from_raw_parts(buf_ptr, len as usize) };
    if let Ok(s) = core::str::from_utf8(bytes) {
        crate::print!("{}", s);
    }
    len
}

/// SYS_GETPID: return the current process ID
fn sys_getpid() -> u64 {
    crate::gdt::get_per_cpu().current_pid as u64
}

/// SYS_EXIT: terminate the current process
fn sys_exit(code: u64) -> ! {
    crate::process::exit(code)
}

/// SYS_KILL: Terminate a process by PID
fn sys_kill(pid: u64) -> u64 {
    match crate::process::kill(pid as usize) {
        Ok(_) => 0,
        Err(_) => u64::MAX, // ESRCH (Process not found)
    }
}
