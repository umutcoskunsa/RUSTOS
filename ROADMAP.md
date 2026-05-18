# Rust OS Development Roadmap 🦀🚀

This roadmap outlines the journey of building an x86_64 operating system entirely from scratch, utilizing a 2-stage custom assembly bootloader and a 64-bit Rust kernel. 

## Phase 1: The Bootloader & CPU Transitions (Completed)
Focus: Booting from disk, setting up memory, and entering 64-bit mode to hand over control to Rust.
- [x] Stage 1 Bootloader: 16-bit Real Mode booting from Floppy/FAT12.
- [x] Stage 2 Loader: Loading the kernel payload from disk.
- [x] Transition to 32-bit Protected Mode (GDT, A20 line).
- [x] Transition to 64-bit Long Mode (Identity Paging, PAE, LM bit).
- [x] Booting into the Rust Kernel entry point and printing output.

## Phase 2: The Rust Kernel Foundation (Completed)
Focus: Establishing the bare-metal Rust environment (`#![no_std]`).
- [x] The VGA Text Mode Driver: Writing a safe wrapper to print to `0xb8000` text memory.
- [x] Safe `println!` and `print!` Macros.
- [x] Serial Port Driver: For debugging and logging to the host machine (QEMU).

## Phase 3: Interrupts and CPU Exceptions (Completed)
Focus: Catching faults and handling hardware input.
- [x] CPU Exceptions: Setting up the Interrupt Descriptor Table (IDT) to catch things like Divide-by-Zero.
- [x] Double Faults: Creating a separate stack (TSS) to catch fatal crashes gracefully.
- [x] Hardware Interrupts (PIC/APIC): Programming the interrupt controller.
- [x] Keyboard Input: Processing raw scancodes into ASCII characters via interrupts.
- [x] Programmable Interval Timer (PIT): Keeping track of time and ticks.

## Phase 4: Memory Management (Completed)
Focus: Managing RAM safely using Rust's ownership model.
- [x] Reading the Memory Map (from BIOS/Bootloader).
- [x] Physical Memory Manager: A frame allocator to track which 4KB chunks of RAM are free.
- [x] Virtual Memory & Paging: Modifying page tables to map physical addresses to virtual addresses safely.
- [x] Heap Allocation: Implementing an allocator (e.g., Linked List or Bump Allocator) to enable the `alloc` crate (`Box`, `Vec`, `String`).

## Phase 5: Advanced Features & Multitasking (Completed)
Focus: Running multiple tasks and improving performance.
- [x] Cooperative Multitasking: Using Rust's `async`/`await` for kernel tasks.
- [x] Preemptive Multitasking: Context switching between threads using timer interrupts.
- [x] APIC and Multicore Support (SMP): Replacing legacy 8259 PIC, parsing ACPI/MADT, waking Application Processors via INIT/SIPI, and Full Per-CPU Metadata (GDT/TSS/Stacks/Scheduler) isolation.

## Phase 6: User Space & Filesystems (Completed)
Focus: Making the OS usable for real applications.
- [x] Disk Drivers (ATA PIO): LBA28 sector reads/writes to a virtual hard disk.
- [x] Virtual File System (FAT32): Custom no_std FAT32 parser — cluster chains, 8.3 filenames, file reads.
- [x] FAT32 Write Support: Cluster allocation, chain linking, directory entry create/overwrite.
- [x] Entering Ring 3 (User Mode): iretq-based privilege drop to Ring 3 with dedicated user stack.
- [x] System Calls (SYSCALL/SYSRET): Configured IA32_STAR/LSTAR/FMASK MSRs, syscall dispatch table.
- [x] The Shell: Interactive `>` prompt with `ls`, `cat`, `cap`, `run`, `help`, `uname`, `clear`.
- [x] cap Text Editor: Full-screen VGA editor with line numbers, cursor, arrow keys, F2/Ctrl+S save, Esc quit.

## Phase 7: Processes & IPC (Completed)
- [x] ELF Binary Loader: Parse and load standard ELF executables from disk into user space.
- [x] Process Isolation: Separate virtual address spaces per process using new page tables.
- [x] Preemptive Multitasking: Per-core Round-Robin scheduler handling isolated Ring 3 processes.
- [x] Inter-Process Communication (IPC): File Descriptor table, `SYS_SPAWN` process cloning, and Ring-Buffer Pipes with Yieldless Blocking.
- [x] Signals & Termination: `SYS_KILL`, `SYS_EXIT`, and `#GP` fault recovery avoiding kernel panics.

## Phase 8: Beyond (📍 WE ARE HERE)
- [x] Network Stack: RTL8139 / VirtIO driver, ARP, IP, UDP, TCP.
- [x] Graphics Engine: VESA/VBE framebuffer, pixel drawing, 8×8 bitmap font, fill_rect, clear.
- [ ] ext2 Filesystem: A real Linux-compatible filesystem.
- [x] Advanced Memory Management: User-space dynamic memory mapping (`mmap` / `sbrk`).
- [x] Port a C standard library (newlib/musl) to run real programs (Implemented custom libc).

## Phase 9: The DOOM Milestone 🎮
- [x] System Call Translation: Map POSIX `open`, `read`, `write`, `lseek`, `mmap` to our MYNEWOS syscalls.
- [x] Keyboard Input: Implement `SYS_GETKEY` to pass raw scancodes to user-space.
- [x] Port `doomgeneric`: Compile the DOOM source code against our C library.
- [x] Load `DOOM1.WAD`: WAD is bundled into the disk image and loaded by the engine.
- [x] Render the Framebuffer: Blit DOOM's internal pixel buffer to our VESA Graphics Engine.
- [x] IT RUNS DOOM!

---
*Reference: This roadmap is heavily inspired by modern Rust bare-metal development patterns, like those found in [Phil Opp's Writing an OS in Rust](https://os.phil-opp.com/).*
