/// Syscall/SYSRET setup and dispatch.
/// Configures the IA32_STAR, IA32_LSTAR, and IA32_FMASK MSRs.
use core::arch::asm;
use alloc::vec::Vec;

// Syscall numbers (Linux-compatible subset)
pub const SYS_READ:      u64 = 0;
pub const SYS_WRITE:     u64 = 1;
pub const SYS_OPEN:      u64 = 2;   // Linux compatible
pub const SYS_CLOSE:     u64 = 3;   // Linux compatible
pub const SYS_LSEEK:     u64 = 8;   // Linux compatible
pub const SYS_MMAP:      u64 = 9;   // Linux-compatible mmap
pub const SYS_PIPE:      u64 = 22;
pub const SYS_GETPID:    u64 = 39;
pub const SYS_GETKEY:    u64 = 40;  // Custom
pub const SYS_SCREEN_BLIT: u64 = 41; // Custom
pub const SYS_GETTICKS:   u64 = 42; // Custom
pub const SYS_EXIT:      u64 = 60;
pub const SYS_KILL:      u64 = 62;
pub const SYS_SBRK:      u64 = 12;  // Linux brk
pub const SYS_RENAME:    u64 = 82;  // Linux compatible
pub const SYS_UNLINK:    u64 = 87;  // Linux compatible
pub const SYS_SPAWN:     u64 = 400; // Custom

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
        //   0x18 = user data    (Ring 3)
        //   0x20 = user code    (Ring 3)
        //   0x28 = TSS
        //
        // SYSRET uses: SS = STAR[63:48]+8, CS = STAR[63:48]+16
        // To get SS=0x18 and CS=0x20, we MUST set STAR[63:48] to 0x10 (Kernel Data).
        let star_kernel: u64 = 0x0008; // Kernel: CS=0x08, SS=0x10
        let star_user:   u64 = 0x0010; // Result: SS=0x18, CS=0x20
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

/// Raw syscall entry point - naked function to preserve all registers exactly.
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
        "and rsp, -16",                   // Ensure 16-byte alignment
        
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
        "mov rsp, gs:[8]",                // Restore user RSP
        "swapgs",                         // Restore user GS base
        "sysretq",
        dispatch = sym dispatch,
    );
}

/// Rust syscall dispatcher - called from the naked handler above.
extern "C" fn dispatch(arg1: u64, arg2: u64, arg3: u64, number: u64) -> u64 {
    match number {
        SYS_READ      => sys_read(arg1, arg2 as *mut u8, arg3),
        SYS_WRITE     => sys_write(arg1, arg2 as *const u8, arg3),
        SYS_OPEN      => sys_open(arg1 as *const u8, arg2 as usize),
        SYS_CLOSE     => sys_close(arg1),
        SYS_LSEEK     => sys_lseek(arg1, arg2, arg3),
        SYS_MMAP      => sys_mmap(arg1, arg2, arg3),
        SYS_SBRK      => sys_sbrk(arg1),
        SYS_PIPE      => sys_pipe(arg1 as *mut u32),
        SYS_SPAWN     => sys_spawn(arg1 as *const u8, arg2 as usize),
        SYS_GETPID    => sys_getpid(),
        SYS_GETKEY    => sys_getkey(),
        SYS_SCREEN_BLIT => sys_screen_blit(arg1 as *const u32, arg2 as u32, arg3 as u32),
        SYS_GETTICKS  => sys_getticks(),
        SYS_EXIT      => sys_exit(arg1),
        SYS_KILL      => sys_kill(arg1),
        SYS_RENAME    => sys_rename(arg1 as *const u8, arg2 as usize, arg3 as *const u8, 0), // simplified
        SYS_UNLINK    => sys_unlink(arg1 as *const u8, arg2 as usize),
        _             => u64::MAX, // ENOSYS
    }
}

// =============================================================================
// SYS_BRK (Linux syscall 12)
// =============================================================================
// Linux brk(addr) sets the break to `addr` and returns the NEW break.
// If addr is 0 or invalid, it returns the CURRENT break.
// =============================================================================
fn sys_sbrk(addr: u64) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();

    let (old_break, cr3) = match table.iter_mut().find(|p| p.pid == current_pid) {
        Some(p) => (p.heap_end, p.cr3),
        None    => return u64::MAX,
    };

    // If addr is 0 or less than the base heap address, return current break
    // Note: our heap base is 0x10_0000_0000 (set in process::spawn)
    if addr <= 0x10_0000_0000 {
        return old_break;
    }

    let new_break = addr;
    let new_break_aligned = (new_break + 0xFFF) & !0xFFF;

    if new_break_aligned == old_break {
        return old_break;
    }

    // Map new pages if the heap is growing
    if new_break_aligned > old_break {
        let pages_needed = (new_break_aligned - old_break) / 4096;
        drop(table); // release lock before mapping

        for i in 0..pages_needed {
            let virt = old_break + i * 4096;
            crate::process::map_user_page_in(cr3, virt);
        }

        // Update process table
        let mut table2 = crate::process::PROCESS_TABLE.lock();
        if let Some(p) = table2.iter_mut().find(|p| p.pid == current_pid) {
            p.heap_end = new_break_aligned;
        }
    } else {
        // Shrinking heap (we just update the pointer, don't actually unmap for now)
        if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
            p.heap_end = new_break_aligned;
        }
    }

    crate::serial_println!("SYS_BRK: PID {} break {:#x} -> {:#x}", current_pid, old_break, new_break_aligned);
    new_break_aligned
}

// =============================================================================
// SYS_MMAP (Linux syscall 9)
// =============================================================================
// Minimal anonymous mmap implementation.
// arg1 = addr hint (ignored), arg2 = length, arg3 = flags
// Returns the virtual address of the new mapping, or u64::MAX on failure.
// MAP_ANONYMOUS (0x20) means no file backing - just zeroed pages.
// This is what musl's malloc uses for large allocations.
// =============================================================================
fn sys_mmap(addr: u64, length: u64, flags: u64) -> u64 {
    const MAP_ANONYMOUS: u64 = 0x20;

    if flags & MAP_ANONYMOUS == 0 {
        // File-backed mmap not yet supported
        crate::serial_println!("SYS_MMAP: File-backed mmap not supported yet.");
        return u64::MAX;
    }

    if length == 0 { return u64::MAX; }

    let current_pid = crate::gdt::get_per_cpu().current_pid;

    // We allocate anonymous mmaps from the process heap break, just like sbrk.
    // A more sophisticated allocator would maintain a separate mmap region.
    let pages = (length + 0xFFF) / 4096;

    let mut table = crate::process::PROCESS_TABLE.lock();
    let proc = match table.iter_mut().find(|p| p.pid == current_pid) {
        Some(p) => p,
        None    => return u64::MAX,
    };

    let base = proc.heap_end;
    let new_end = base + pages * 4096;
    let cr3 = proc.cr3;
    proc.heap_end = new_end;
    drop(table);

    for i in 0..pages {
        crate::process::map_user_page_in(cr3, base + i * 4096);
    }

    crate::serial_println!("SYS_MMAP: PID {} anon map {:#x}..{:#x} ({} pages)", current_pid, base, new_end, pages);
    base
}
/// SYS_PIPE: Create a new pipe and return two file descriptors in the given array pointer.
/// `fds_ptr` should point to an array of two u32s.
fn sys_pipe(fds_ptr: *mut u32) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
        let (read_fd, write_fd) = crate::ipc::create_pipe();
        
        let mut fd1 = None;
        let mut fd2 = None;
        
        // Find two empty slots
        for (i, fd) in p.fd_table.iter_mut().enumerate() {
            if fd.is_none() {
                if fd1.is_none() {
                    fd1 = Some(i);
                    *fd = Some(read_fd.clone());
                } else if fd2.is_none() {
                    fd2 = Some(i);
                    *fd = Some(write_fd.clone());
                    break;
                }
            }
        }
        
        // If we didn't find empty slots, push new ones
        let fd1 = fd1.unwrap_or_else(|| {
            let i = p.fd_table.len();
            p.fd_table.push(Some(read_fd));
            i
        });
        
        let fd2 = fd2.unwrap_or_else(|| {
            let i = p.fd_table.len();
            p.fd_table.push(Some(write_fd));
            i
        });
        
        // Write the FDs to user memory
        unsafe {
            let slice = core::slice::from_raw_parts_mut(fds_ptr, 2);
            slice[0] = fd1 as u32;
            slice[1] = fd2 as u32;
        }
        return 0; // Success
    }
    u64::MAX
}

/// SYS_READ: Read `len` bytes into `buf_ptr` from `fd`
fn sys_read(fd: u64, buf_ptr: *mut u8, len: u64) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
        if (fd as usize) >= p.fd_table.len() { return u64::MAX; }
        
        let fd_obj_clone = p.fd_table[fd as usize].clone();
        drop(table);
        
        if let Some(fd_obj) = fd_obj_clone {
            let bytes = unsafe { core::slice::from_raw_parts_mut(buf_ptr, len as usize) };
            match fd_obj {
                crate::ipc::FileDescriptor::PipeRead(pipe) => {
                    let mut pipe_guard = pipe.lock();
                    if pipe_guard.buffer.is_empty() {
                        // Set state to Blocked so we don't waste CPU cycles.
                        // We will return u64::MAX (EAGAIN) so user-space knows to retry when it wakes up.
                        // (We need to re-acquire the table lock for this)
                        drop(pipe_guard);
                        let mut table2 = crate::process::PROCESS_TABLE.lock();
                        if let Some(p2) = table2.iter_mut().find(|p| p.pid == current_pid) {
                            p2.state = crate::process::ProcessState::Blocked;
                        }
                        return u64::MAX;
                    }
                    
                    let mut read_bytes = 0;
                    for i in 0..(len as usize) {
                        if let Some(b) = pipe_guard.buffer.pop_front() {
                            bytes[i] = b;
                            read_bytes += 1;
                        } else {
                            break;
                        }
                    }

                    // Wake up any blocked processes (they might be waiting to write)
                    drop(pipe_guard);
                    let mut table2 = crate::process::PROCESS_TABLE.lock();
                    for p2 in table2.iter_mut() {
                        if p2.state == crate::process::ProcessState::Blocked {
                            p2.state = crate::process::ProcessState::Ready;
                        }
                    }

                    return read_bytes as u64;
                },
                crate::ipc::FileDescriptor::File(file) => {
                    let mut file_guard = file.lock();
                    let offset = file_guard.offset;
                    let data = &file_guard.data;
                    
                    if offset >= data.len() { return 0; } // EOF
                    
                    let to_read = core::cmp::min(len as usize, data.len() - offset);
                    bytes[..to_read].copy_from_slice(&data[offset..offset + to_read]);
                    
                    file_guard.offset += to_read;
                    return to_read as u64;
                },
                _ => return u64::MAX, // Can't read from write-only descriptors
            }
        }
    }
    u64::MAX
}

/// SYS_WRITE: write `len` bytes from `buf_ptr` to fd
fn sys_write(fd: u64, buf_ptr: *const u8, len: u64) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
        if (fd as usize) >= p.fd_table.len() { return u64::MAX; }
        
        // Clone the FD reference to avoid holding the lock while doing IO
        let fd_obj_clone = p.fd_table[fd as usize].clone();
        
        // We can drop the process table lock before printing to screen to avoid deadlocks
        drop(table);
        
        if let Some(fd_obj) = fd_obj_clone {
            let bytes = unsafe { core::slice::from_raw_parts(buf_ptr, len as usize) };
            match fd_obj {
                crate::ipc::FileDescriptor::StandardOutput | crate::ipc::FileDescriptor::StandardError => {
                    if let Ok(s) = core::str::from_utf8(bytes) {
                        crate::print!("{}", s);
                    }
                    return len;
                },
                crate::ipc::FileDescriptor::PipeWrite(pipe) => {
                    let mut pipe_guard = pipe.lock();
                    if pipe_guard.buffer.len() >= pipe_guard.max_size {
                        // Pipe is full, block the writer
                        drop(pipe_guard);
                        let mut table2 = crate::process::PROCESS_TABLE.lock();
                        if let Some(p2) = table2.iter_mut().find(|p| p.pid == current_pid) {
                            p2.state = crate::process::ProcessState::Blocked;
                        }
                        return u64::MAX;
                    }

                    let mut written = 0;
                    for &b in bytes {
                        if pipe_guard.buffer.len() < pipe_guard.max_size {
                            pipe_guard.buffer.push_back(b);
                            written += 1;
                        } else {
                            break;
                        }
                    }
                    
                    // Wake up any blocked processes (they might be waiting to read)
                    drop(pipe_guard);
                    let mut table2 = crate::process::PROCESS_TABLE.lock();
                    for p2 in table2.iter_mut() {
                        if p2.state == crate::process::ProcessState::Blocked {
                            p2.state = crate::process::ProcessState::Ready;
                        }
                    }
                    
                    return written;
                },
                crate::ipc::FileDescriptor::File(file) => {
                    // For now, file writes are not persistent beyond this process's memory
                    // unless we call a sync/flush syscall. We'll just update the buffer.
                    let mut file_guard = file.lock();
                    let offset = file_guard.offset;
                    
                    // If writing past end, extend
                    if offset + bytes.len() > file_guard.data.len() {
                        file_guard.data.resize(offset + bytes.len(), 0);
                    }
                    
                    file_guard.data[offset..offset + bytes.len()].copy_from_slice(bytes);
                    file_guard.offset += bytes.len();
                    return len;
                },
                _ => return u64::MAX, // Can't write to read-only descriptors
            }
        }
    }
    u64::MAX
}

/// SYS_OPEN: Open a file and return a new FD
fn sys_open(name_ptr: *const u8, name_len: usize) -> u64 {
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    if let Ok(name) = core::str::from_utf8(name_bytes) {
        // Try to read existing file; if missing, create empty state for a new file
        let data = crate::fs::read_file(name).unwrap_or_else(Vec::new);
        
        let file_state = crate::ipc::FileState {
            data,
            offset: 0,
            name: alloc::string::String::from(name),
        };
        let fd_obj = crate::ipc::FileDescriptor::File(alloc::sync::Arc::new(spin::Mutex::new(file_state)));
        
        let current_pid = crate::gdt::get_per_cpu().current_pid;
        let mut table = crate::process::PROCESS_TABLE.lock();
        if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
            // Find empty slot or push
            for (i, slot) in p.fd_table.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(fd_obj);
                    return i as u64;
                }
            }
            let i = p.fd_table.len();
            p.fd_table.push(Some(fd_obj));
            return i as u64;
        }
    }
    u64::MAX
}

/// SYS_CLOSE: Close a file descriptor
fn sys_close(fd: u64) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
        if (fd as usize) < p.fd_table.len() {
            let fd_obj = p.fd_table[fd as usize].take();
            drop(table);

            if let Some(crate::ipc::FileDescriptor::File(file)) = fd_obj {
                // Persistent write on close!
                let file_guard = file.lock();
                crate::fs::write_file(&file_guard.name, &file_guard.data);
            }
            return 0;
        }
    }
    u64::MAX
}

/// SYS_LSEEK: Change current file offset
/// arg1=fd, arg2=offset, arg3=whence (0=SET, 1=CUR, 2=END)
fn sys_lseek(fd: u64, offset: u64, whence: u64) -> u64 {
    let current_pid = crate::gdt::get_per_cpu().current_pid;
    let mut table = crate::process::PROCESS_TABLE.lock();
    if let Some(p) = table.iter_mut().find(|p| p.pid == current_pid) {
        if (fd as usize) >= p.fd_table.len() { return u64::MAX; }
        if let Some(crate::ipc::FileDescriptor::File(file)) = &p.fd_table[fd as usize] {
            let mut file_guard = file.lock();
            let new_offset = match whence {
                0 => offset as i64,                       // SEEK_SET
                1 => file_guard.offset as i64 + offset as i64, // SEEK_CUR
                2 => file_guard.data.len() as i64 + offset as i64, // SEEK_END
                _ => return u64::MAX,
            };
            
            if new_offset < 0 { return u64::MAX; }
            file_guard.offset = new_offset as usize;
            return file_guard.offset as u64;
        }
    }
    u64::MAX
}

/// SYS_SPAWN: Create a new process from an ELF file, inheriting File Descriptors
fn sys_spawn(name_ptr: *const u8, name_len: usize) -> u64 {
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    if let Ok(name) = core::str::from_utf8(name_bytes) {
        // Read file from FS
        if let Some(file_data) = crate::fs::read_file(name) {
            // Spawn the process
            if let Ok(new_pid) = crate::process::spawn(&file_data, name) {
                // Now copy the FD table from the current process to the new process
                let current_pid = crate::gdt::get_per_cpu().current_pid;
                let mut table = crate::process::PROCESS_TABLE.lock();
                
                let mut cloned_fd_table = None;
                
                // Get parent FD table
                if let Some(parent) = table.iter().find(|p| p.pid == current_pid) {
                    cloned_fd_table = Some(parent.fd_table.clone());
                }
                
                // Assign to child
                if let Some(fd_table) = cloned_fd_table {
                    if let Some(child) = table.iter_mut().find(|p| p.pid == new_pid) {
                        child.fd_table = fd_table;
                    }
                }
                
                return new_pid as u64;
            }
        }
    }
    u64::MAX // Error
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

/// SYS_SCREEN_BLIT: Copy a 32-bit pixel buffer to the center of the screen.
/// arg1=buf_ptr, arg2=width, arg3=height
fn sys_screen_blit(buf_ptr: *const u32, w: u32, h: u32) -> u64 {
    if !crate::graphics::is_active() { return u64::MAX; }
    
    let screen_w = crate::graphics::width();
    let screen_h = crate::graphics::height();
    
    // Safety check: ensure we don't read past the buffer
    let buffer = unsafe { core::slice::from_raw_parts(buf_ptr, (w * h) as usize) };
    
    // Center the image
    let start_x = (screen_w.saturating_sub(w)) / 2;
    let start_y = (screen_h.saturating_sub(h)) / 2;
    
    for y in 0..h {
        if start_y + y >= screen_h { break; }
        for x in 0..w {
            if start_x + x >= screen_w { break; }
            let color = buffer[(y * w + x) as usize];
            crate::graphics::put_pixel(start_x + x, start_y + y, color);
        }
    }
    
    0
}

/// SYS_GETTICKS: Get system uptime in milliseconds
fn sys_getticks() -> u64 {
    crate::interrupts::TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed) * 10
}

/// SYS_GETKEY: Get next raw scancode from keyboard buffer. Returns 0 if empty.
fn sys_getkey() -> u64 {
    crate::task::keyboard::pop_scancode().unwrap_or(0) as u64
}

/// SYS_UNLINK: Delete a file from the root directory.
fn sys_unlink(name_ptr: *const u8, name_len: usize) -> u64 {
    let name_bytes = unsafe { core::slice::from_raw_parts(name_ptr, name_len) };
    if let Ok(name) = core::str::from_utf8(name_bytes) {
        if crate::fs::delete_file(name) {
            return 0;
        }
    }
    u64::MAX
}

/// SYS_RENAME: Rename a file (simplified).
fn sys_rename(old_ptr: *const u8, old_len: usize, new_ptr: *const u8, new_len: usize) -> u64 {
    let old_bytes = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
    if let Ok(old_name) = core::str::from_utf8(old_bytes) {
        // Find new name (assume it's null-terminated for simplicity)
        let mut n_len = 0;
        unsafe {
            while *new_ptr.add(n_len) != 0 && n_len < 256 {
                n_len += 1;
            }
        }
        let new_bytes = unsafe { core::slice::from_raw_parts(new_ptr, n_len) };
        if let Ok(new_name) = core::str::from_utf8(new_bytes) {
            if crate::fs::rename_file(old_name, new_name) {
                return 0;
            }
        }
    }
    u64::MAX
}

