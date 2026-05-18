; =============================================================================
; syscall.S — Raw system call wrappers for MYNEWOS x86_64 user-space
;
; Calling convention (x86_64 System V):
;   Arguments:  rdi, rsi, rdx, r10, r8, r9
;   Syscall #:  rax
;   Return:     rax
;
; Our kernel syscall numbers (Linux-compatible subset):
;   0  = read   1  = write  2  = open
;   3  = close  8  = lseek  9  = mmap
;   12 = brk    22 = pipe   39 = getpid
;   60 = exit   62 = kill   
;   400 = spawn (custom)
; =============================================================================

section .text

; long syscall0(long n)
global syscall0
syscall0:
    mov rax, rdi
    syscall
    ret

; long syscall1(long n, long a1)
global syscall1
syscall1:
    mov rax, rdi
    mov rdi, rsi
    syscall
    ret

; long syscall2(long n, long a1, long a2)
global syscall2
syscall2:
    mov rax, rdi
    mov rdi, rsi
    mov rsi, rdx
    syscall
    ret

; long syscall3(long n, long a1, long a2, long a3)
global syscall3
syscall3:
    mov rax, rdi
    mov rdi, rsi
    mov rsi, rdx
    mov rdx, rcx
    syscall
    ret

; long syscall6(long n, long a1, long a2, long a3, long a4, long a5, long a6)
global syscall6
syscall6:
    ; On x86_64 SysV: n=rdi, a1=rsi, a2=rdx, a3=rcx, a4=r8, a5=r9, a6=stack
    ; For 6-arg syscall: rax=n, rdi=a1, rsi=a2, rdx=a3, r10=a4, r8=a5, r9=a6
    mov rax, rdi
    mov rdi, rsi
    mov rsi, rdx
    mov rdx, rcx
    mov r10, r8
    mov r8,  r9
    syscall
    ret

; =============================================================================
; Standard POSIX wrappers
; =============================================================================

; ssize_t write(int fd, const void *buf, size_t count)
global write
write:
    mov rax, 1
    syscall
    ret

; ssize_t read(int fd, void *buf, size_t count)
global read
read:
    mov rax, 0
    syscall
    ret

; void *brk(long addr) — extends heap break (our sbrk handler)
global brk
brk:
    mov rax, 12
    syscall
    ret

; void *mmap(void *addr, size_t len, int prot, int flags, int fd, off_t off)
; For anonymous mmap: mmap(NULL, len, PROT_RW, MAP_ANON|MAP_PRIVATE, -1, 0)
; We pack: rdi=addr, rsi=len, rdx=flags — kernel only checks len+flags for anon
global mmap
mmap:
    ; C args: addr=rdi, len=rsi, prot=rdx, flags=rcx, fd=r8, off=r9
    ; Kernel dispatch(arg1=addr, arg2=len, arg3=flags, num=9)
    mov rax, 9
    mov rdx, rcx   ; flags (4th C arg) -> rdx (kernel arg3)
    syscall
    ret

; int getpid(void)
global getpid
getpid:
    mov rax, 39
    syscall
    ret

; void _exit(int status) — raw exit without atexit handlers
global _exit
_exit:
    mov rax, 60
    syscall
    ud2              ; should not reach here

; int open(const char *name, int flags, ...)
global open
open:
    ; We currently ignore flags, kernel expects: rdi=name, rsi=len
    ; We need to calculate length of name
    push rdi
    xor al, al
    mov rcx, -1
    repne scasb
    not rcx
    dec rcx
    mov rsi, rcx   ; len
    pop rdi        ; name
    mov rax, 2
    syscall
    ret

; int close(int fd)
global close
close:
    mov rax, 3
    syscall
    ret

; long lseek(int fd, long offset, int whence)
global lseek
lseek:
    mov rax, 8
    syscall
    ret

; int getkey(void)
global getkey
getkey:
    mov rax, 40
    syscall
    ret

; int screen_blit(const void *buf, int w, int h)
global screen_blit
screen_blit:
    mov rax, 41
    syscall
    ret

; long getticks(void)
global getticks
getticks:
    mov rax, 42
    syscall
    ret

; int unlink(const char *name)
global unlink
unlink:
    push rdi
    xor al, al
    mov rcx, -1
    repne scasb
    not rcx
    dec rcx
    mov rsi, rcx   ; len
    pop rdi        ; name
    mov rax, 87
    syscall
    ret

; int rename(const char *oldname, const char *newname)
global rename
rename:
    push rsi       ; Save newname
    push rdi       ; Save oldname
    xor al, al
    mov rcx, -1
    repne scasb
    not rcx
    dec rcx
    mov rsi, rcx   ; old_len
    pop rdi        ; old_name
    pop rdx        ; new_name
    mov rax, 82
    syscall
    ret

section .note.GNU-stack noalloc noexec nowrite progbits
