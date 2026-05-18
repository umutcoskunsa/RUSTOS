#![no_std]
#![no_main]

use core::arch::asm;

fn sys_write(fd: u32, msg: &[u8]) {
    unsafe {
        asm!(
            "syscall",
            in("rax") 1u64,
            in("rdi") fd as u64,
            in("rsi") msg.as_ptr() as u64,
            in("rdx") msg.len() as u64,
            lateout("rax") _,
            out("rcx") _, out("r11") _,
            options(nostack)
        );
    }
}

fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") 60u64, // SYS_EXIT
            in("rdi") code,
            options(noreturn)
        );
    }
}

fn print(s: &str) {
    sys_write(1, s.as_bytes());
}

fn sleep() {
    let mut i: u64 = 0;
    while i < 20_000_000 {
        unsafe { asm!("nop", options(nomem, nostack, preserves_flags)); }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("CHILD: Woke up!\n");
    print("CHILD: Doing some hard work (sleeping)...\n");
    sleep();
    
    let msg = b"Hello from the Child Process!";
    print("CHILD: Writing to Pipe FD 4...\n");
    sys_write(4, msg); // We assume the write end of the pipe is 4
    
    print("CHILD: Exiting.\n");
    sys_exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys_exit(1);
}
