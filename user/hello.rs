// MYNEWOS — Hello World user-space program
// Compiled with: rustc --edition 2021 -C panic=abort --target x86_64-unknown-none -o hello.elf user/hello.rs
#![no_std]
#![no_main]

// This runs in Ring 3 with no standard library.
// It communicates with the kernel via the SYSCALL instruction.

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let msg = b"Hello from Ring 3 (user space)!\n";
    sys_write(1, msg.as_ptr(), msg.len());
    sys_exit(0);
}

fn sys_write(fd: u64, buf: *const u8, len: usize) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 1u64,       // SYS_WRITE = 1
            in("rdi") fd,
            in("rsi") buf as u64,
            in("rdx") len as u64,
            out("rcx") _,         // clobbered by SYSCALL
            out("r11") _,         // clobbered by SYSCALL
            lateout("rax") ret,
        );
    }
    ret
}

fn sys_exit(code: i64) -> ! {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 60u64,      // SYS_EXIT = 60
            in("rdi") code as u64,
            options(noreturn)
        );
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
