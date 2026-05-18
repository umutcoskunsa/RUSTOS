; =============================================================================
; crt0.S — C Runtime entry point for MYNEWOS
;
; This is the first code that runs in a C program. It sets up the environment
; (though we don't have much to set up yet) and calls main().
; =============================================================================

section .text
global _start
extern main
extern exit

_start:
    ; The kernel passes control here. 
    ; stack is already set up by the kernel ELF loader.
    
    ; 1. Call main
    ; argc, argv would be passed here if we had them.
    xor rdi, rdi    ; argc = 0
    xor rsi, rsi    ; argv = NULL
    call main
    
    ; 2. Call exit with the return value of main
    mov rdi, rax
    call exit
    
    ; 3. Safety catch
    ud2

section .note.GNU-stack noalloc noexec nowrite progbits
