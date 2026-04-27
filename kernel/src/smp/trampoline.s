// 16-bit Real Mode trampoline for Application Processors (APs)
// Written in AT&T syntax (GNU AS default used by Rust global_asm!)
// Copied to physical address 0x8000 and executed by each sleeping AP
// after the BSP sends INIT + SIPI Inter-Processor Interrupts.

// Switch to AT&T syntax for this file (context_switch.s sets Intel mode globally)
.att_syntax prefix

.code16
.global ap_trampoline_start
.global ap_trampoline_end

ap_trampoline_start:
    cli
    cld

    // Set up segment registers (we're executing at physical 0x8000)
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    // Load temporary GDT (address relative to load base 0x8000)
    lgdtl (ap_gdt_descriptor - ap_trampoline_start + 0x8000)

    // Enter Protected Mode: set CR0.PE (bit 0)
    movl %cr0, %eax
    orl  $1, %eax
    movl %eax, %cr0

    // Far jump to flush prefetch queue and enter 32-bit code
    ljmpl $0x08, $(ap_protected_mode - ap_trampoline_start + 0x8000)

.code32
ap_protected_mode:
    movw $0x10, %ax
    movw %ax,   %ds
    movw %ax,   %es
    movw %ax,   %ss

    // Enable PAE (required for Long Mode entry)
    movl %cr4, %eax
    orl  $(1 << 5), %eax
    movl %eax, %cr4

    // Load BSP's CR3 (page tables) written by Rust at 0x7FF8
    movl 0x7FF8, %eax
    movl %eax, %cr3

    // Set EFER.LME bit (enable Long Mode in MSR 0xC0000080)
    movl $0xC0000080, %ecx
    rdmsr
    orl  $(1 << 8), %eax
    wrmsr

    // Enable paging to activate Long Mode
    movl %cr0, %eax
    orl  $(1 << 31), %eax
    movl %eax, %cr0

    // Far jump into 64-bit Long Mode
    ljmpl $0x18, $(ap_long_mode_entry - ap_trampoline_start + 0x8000)

.code64
ap_long_mode_entry:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    // Set up a temporary stack for this AP (below trampoline code)
    movq $0x7000, %rsp

    // Load the shared IDT written by BSP at 0x7FE8
    lidtq 0x7FE8

    // Call the Rust ap_entry function (address written by BSP at 0x7FD8)
    callq *0x7FD8

.ap_halt:
    hlt
    jmp .ap_halt

// Temporary GDT for the AP trampoline (flat segments)
.align 8
ap_gdt:
    .quad 0x0000000000000000   // Null
    .quad 0x00CF9A000000FFFF   // 32-bit Code (index 1 -> selector 0x08)
    .quad 0x00CF92000000FFFF   // 32-bit Data (index 2 -> selector 0x10)
    .quad 0x00AF9A000000FFFF   // 64-bit Code (index 3 -> selector 0x18)
    .quad 0x00AF92000000FFFF   // 64-bit Data (index 4 -> selector 0x20)

ap_gdt_descriptor:
    .word ap_gdt_descriptor - ap_gdt - 1       // GDT Limit
    .long ap_gdt - ap_trampoline_start + 0x8000 // GDT Physical Base

ap_trampoline_end:

// Restore Intel syntax for the rest of the kernel assembly
.intel_syntax noprefix
