#![no_std]
#![no_main]

use core::arch::asm;

fn sys_pipe(fds: &mut [u32; 2]) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 22u64,
            in("rdi") fds.as_mut_ptr() as u64,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
            options(nostack)
        );
    }
    ret
}

fn sys_spawn(name: &str) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 400u64,
            in("rdi") name.as_ptr() as u64,
            in("rsi") name.len() as u64,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
            options(nostack)
        );
    }
    ret
}

fn sys_read(fd: u32, buf: &mut [u8]) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "syscall",
            in("rax") 0u64,
            in("rdi") fd as u64,
            in("rsi") buf.as_mut_ptr() as u64,
            in("rdx") buf.len() as u64,
            lateout("rax") ret,
            out("rcx") _, out("r11") _,
            options(nostack)
        );
    }
    ret
}

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

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("PARENT: Starting up...\n");

    let mut fds = [0u32; 2];
    sys_pipe(&mut fds);
    // fds[0] = read end (likely 3)
    // fds[1] = write end (likely 4)

    print("PARENT: Spawning child.elf...\n");
    sys_spawn("child.elf");

    print("PARENT: Waiting for message from child via Pipe...\n");

    let mut buf = [0u8; 32];
    loop {
        let bytes_read = sys_read(fds[0], &mut buf);
        if bytes_read != u64::MAX && bytes_read > 0 {
            print("PARENT: Received message: ");
            sys_write(1, &buf[0..(bytes_read as usize)]);
            print("\n");
            break;
        }
    }

    print("PARENT: Exiting.\n");
    sys_exit(0);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys_exit(1);
}
