#![no_std]
#![no_main]

use core::arch::asm;

fn sleep() {
    // Use a volatile counter so the compiler cannot optimize this away
    let mut i: u64 = 0;
    while i < 10_000_000 {
        unsafe { 
            asm!("nop", options(nomem, nostack, preserves_flags)); 
        }
        i += 1;
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

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let msg = b"Looping in background...\n";
    
    loop {
        sleep();
        
        unsafe {
            asm!(
                "syscall",
                in("rax") 1u64,       // SYS_WRITE
                in("rdi") 1u64,       // fd = stdout
                in("rsi") msg.as_ptr() as u64,
                in("rdx") msg.len() as u64,
                lateout("rax") _,
                out("rcx") _,        // clobbered by syscall
                out("r11") _,        // clobbered by syscall
                options(nostack)
            );
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys_exit(1);
}
