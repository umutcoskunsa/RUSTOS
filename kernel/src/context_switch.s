.intel_syntax noprefix
.global timer_interrupt_handler_asm

timer_interrupt_handler_asm:
    # 1. Hardware has already pushed: SS, RSP, RFLAGS, CS, RIP
    
    # 2. Push all general purpose registers
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    # 3. SWAPGS Dance: If we came from Ring 3, we must swap GS to get our PerCpu pointer
    # CS is at [rsp + 16*8] (15 GPRs)
    # Offset: 16 regs * 8 = 128 bytes.
    # [rsp + 120] = RIP, [rsp + 128] = CS
    test qword ptr [rsp + 128], 3
    jz .skip_swap_entry
    swapgs
.skip_swap_entry:

    # 4. Call the Rust scheduler function
    # Pass current RSP as the first argument (RDI)
    mov rdi, rsp
    # Ensure stack is 16-byte aligned before calling Rust
    mov rbp, rsp          # save old rsp
    and rsp, -16
    call process_schedule
    mov rsp, rbp          # restore old rsp (actually we'll overwrite it anyway)
    
    # 5. RAX contains the new RSP returned by schedule_timer! Update our stack pointer.
    mov rsp, rax
    
    # 6. SWAPGS Dance (Exit): Are we returning to Ring 3?
    # New stack's CS is also at [rsp + 128]
    test qword ptr [rsp + 128], 3
    jz .skip_swap_exit
    swapgs
.skip_swap_exit:

    # 7. Pop all general purpose registers
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    # 8. Return from interrupt
    iretq
