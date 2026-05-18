.global timer_interrupt_handler_asm

timer_interrupt_handler_asm:
    # 1. Hardware has already pushed: SS, RSP, RFLAGS, CS, RIP
    # We push a "dummy" 0 error code to make the frame look like an exception frame (6 values)
    push 0
    # Push a sentinel to verify alignment and offset in Rust (32-bit immediate)
    push 0x12345678
    
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

    # 3. SWAPGS Dance: If we came from Ring 3, we must swap GS
    # Frame is now 6 values (48 bytes) + 15 GPRs (120 bytes) = 168 bytes.
    # CS is at [rsp + 15*8 + 8] (RIP is at 15*8 + 0)
    # Offset: 120 + 8 = 128 bytes.
    # [rsp + 128] = RIP, [rsp + 136] = CS
    test qword ptr [rsp + 136], 3
    jz .skip_swap_entry
    swapgs
.skip_swap_entry:

    # 4. Call the Rust scheduler function
    # Pass current RSP as the first argument (RDI)
    mov rdi, rsp
    call process_schedule
    
    # 5. RAX contains the new RSP returned by schedule_timer! Update our stack pointer.
    mov rsp, rax
    
    # 6. SWAPGS Dance (Exit): Are we returning to Ring 3?
    # New stack's CS is also at [rsp + 136]
    test qword ptr [rsp + 136], 3
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

    # 8. Clean up sentinel, dummy error code and return
    add rsp, 16
    
    .global context_switch_iretq
context_switch_iretq:
    iretq
