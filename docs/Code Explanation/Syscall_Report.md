# Syscall Implementation Report (C Library Support)

This report explains the system calls implemented to support the C standard library and general user-space execution in MYNEWOS.

## 1. The Syscall Mechanism
MYNEWOS uses the modern x86_64 `SYSCALL` and `SYSRET` instructions.
- **Entry**: The user program places the syscall number in `RAX` and arguments in `RDI`, `RSI`, `RDX`, `R10`, `R8`, `R9`.
- **Return**: The kernel places the result back in `RAX`.

## 2. Core Syscalls implemented

### 2.1 Memory Management (The Engine for `malloc`)
| Syscall | Number | Description |
|---|---|---|
| `SYS_BRK` | 12 | Sets the "Program Break" (the end of the heap). If the new address is higher than the current one, the kernel allocates new pages. |
| `SYS_MMAP` | 9 | Maps memory regions. Currently used for large anonymous allocations (MAP_ANON). |

**How `malloc()` works in MYNEWOS:**
When you call `malloc(size)`:
1. The libc allocator checks its free list.
2. If empty, it calls `brk(new_addr)` to ask the kernel for more heap space.
3. The kernel catches the syscall, allocates physical frames, maps them to the process's page table, and returns the new break.

### 2.2 File and Terminal I/O (The Engine for `printf`)
| Syscall | Number | Description |
|---|---|---|
| `SYS_WRITE` | 1 | Writes bytes from a buffer to a file descriptor. `fd=1` is standard output (serial console/VGA). |
| `SYS_READ` | 0 | Reads bytes from a file descriptor into a buffer. |
| `SYS_OPEN` | 300 | Custom MYNEWOS syscall to open a file from the FAT32 disk and return a handle. |

**How `printf()` works in MYNEWOS:**
1. `printf` formats your string into a local buffer using `vsnprintf`.
2. It then calls the `write(1, buffer, len)` syscall.
3. The kernel receives the buffer and routes it to the `serial` and `vga` drivers.

### 2.3 Process Control
| Syscall | Number | Description |
|---|---|---|
| `SYS_EXIT` | 60 | Terminates the current process and returns an exit code to the parent. |
| `SYS_GETPID` | 39 | Returns the unique Process ID of the caller. |
| `SYS_SPAWN` | 400 | Custom syscall to load a new ELF binary from disk and start a new process. |

## 3. Assembly Bridge (`syscall.S`)
Because C functions cannot execute the `syscall` instruction directly, we provide a "stub" in assembly:

```nasm
global write
write:
    mov rax, 1      ; SYS_WRITE
    syscall         ; Trigger kernel
    ret             ; Return to C caller
```

This bridge ensures that our C library looks and feels like a standard POSIX environment, which is the key requirement for porting **DOOM**.
