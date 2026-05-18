# MYNEWOS — Development Report: Phases 5 & 6

**Project:** MYNEWOS — A bare-metal x86_64 Operating System written in Rust  
**Phases Covered:** Phase 5 (Advanced Multitasking) & Phase 6 (User Space & Filesystems)  
**Architecture:** x86_64, custom 2-stage FAT12 bootloader, Rust `#![no_std]` kernel

---

## Phase 5: Advanced Multitasking

### 5.1 — Cooperative Multitasking (`async`/`await`)

**Problem:** The kernel needed a way to run multiple I/O tasks concurrently without blocking the CPU.

**Solution:** Implemented a full Rust `async`/`await` executor from scratch.

**Files created:**
| File | Purpose |
|------|---------|
| `kernel/src/task/mod.rs` | `Task` struct wrapping a pinned `Future`, unique `TaskId` via atomic counter |
| `kernel/src/task/executor.rs` | `Executor` with a `BTreeMap` task queue; polls tasks with `hlt` when idle |
| `kernel/src/task/keyboard.rs` | Lock-free `ScancodeStream` backed by a `crossbeam_queue::ArrayQueue<u8>`; interrupt handler pushes bytes, async tasks `await` them via `AtomicWaker` |

**Key design:** The interrupt handler (`keyboard_interrupt_handler`) no longer blocks — it pushes raw scancodes into the queue and wakes any sleeping async task. The executor uses `enable_and_hlt` to sleep the CPU when no tasks are runnable, giving zero idle CPU usage.

**Dependencies added:** `crossbeam-queue`, `conquer-once`, `futures-util`

---

### 5.2 — Preemptive Multitasking (Timer-based Context Switching)

**Problem:** Cooperative tasks that spin in an infinite loop would starve all other work. The kernel needed to forcibly share CPU time.

**Solution:** Wrote a **raw x86_64 assembly context switcher** hooked directly into the timer interrupt, implementing Round-Robin preemptive scheduling.

**Files created:**
| File | Purpose |
|------|---------|
| `kernel/src/context_switch.s` | Intel-syntax `global_asm!` routine that pushes all 15 GP registers, calls `schedule_timer()`, swaps RSP from the return value, pops registers from the new stack, and executes `iretq` |
| `kernel/src/task/thread.rs` | `Thread` struct holding a `ThreadId` and `stack_ptr`; `ThreadContext` layout matching the exact register push order; 8KB stack allocated from the kernel heap |
| `kernel/src/task/scheduler.rs` | `Scheduler` with a `VecDeque<Thread>` ready queue; Round-Robin `schedule()` that swaps RSP; `schedule_timer()` as the `#[unsafe(no_mangle)]` C-callable entry point |

**Critical insight:** The x86 `x86-interrupt` ABI cannot manipulate RSP to switch threads. Instead, the assembly stub saves the old RSP as a function argument and receives the new RSP as the integer return value (in `RAX`), then executes `mov rsp, rax` before poping registers.

**Spinlock hardening:** All shared locks (`vga_buffer`, `serial`, `LinkedListAllocator`) were wrapped in `x86_64::instructions::interrupts::without_interrupts(|| { ... })` closures to prevent **deadlocks** when a thread is preempted while holding a lock.

---

### 5.3 — APIC & Symmetric Multiprocessing (SMP)

**Problem:** The kernel was running on a single core using the 1980s 8259 PIC. Modern hardware has multiple CPU cores and uses the Advanced Programmable Interrupt Controller (APIC).

**Solution:** Parsed ACPI tables to locate APIC hardware, disabled the legacy PIC, configured the Local APIC timer, and wrote a **16-bit real-mode trampoline** to wake all sleeping Application Processors (APs).

**Files created:**
| File | Purpose |
|------|---------|
| `kernel/src/apic/acpi_handler.rs` | Implements `acpi::AcpiHandler` to identity-map physical memory regions for ACPI table parsing |
| `kernel/src/apic/mod.rs` | Scans BIOS area (`0xE0000–0xFFFFF`) for the RSDP; parses the MADT via the `acpi` crate; counts CPUs via `PlatformInfo`; masks the 8259 PIC; enables the LAPIC via its Spurious Vector Register; configures APIC Timer in periodic mode; routes Keyboard IRQ 1 through the IOAPIC to Vector 33 |
| `kernel/src/smp/trampoline.s` | AT&T syntax 16-bit assembly blob: Real Mode → Protected Mode → 64-bit Long Mode; inherits BSP's CR3 (page tables) from a shared memory slot; loads IDT; jumps to Rust `ap_entry()` |
| `kernel/src/smp/mod.rs` | Copies trampoline to physical `0x8000`; writes CR3, IDT, and entry point to data slots; sends `INIT + SIPI + SIPI` IPIs via LAPIC ICR; counts online APs via `AP_COUNT` atomic |

**QEMU validation output:**
```
APIC: Local APIC base address: 0xFEE00000
APIC: Total CPU count: 2
APIC: Legacy 8259 PIC masked.
APIC: Local APIC enabled!
APIC: IOAPIC configured for Keyboard (IRQ 1 -> Vector 33).
SMP: Trampoline copied (174 bytes) to 0x8000
SMP: Application Processor 1 is ONLINE!
```

> The AP printed its online message **before** the BSP finished logging the SIPI send — demonstrating true parallel CPU execution.

---

## Phase 6: User Space & Filesystems

### 6.1 — ATA/IDE PIO Disk Driver

**Problem:** The kernel could only read from the boot floppy. There was no way to access persistent storage.

**Solution:** Wrote a complete **ATA PIO (Programmed I/O) LBA28** disk driver communicating directly with I/O ports `0x1F0–0x1F7`.

**File:** `kernel/src/disk/ata.rs` and `kernel/src/disk/mod.rs`

**Implementation details:**
- Selects master drive with `0xE0 | LBA[27:24]` in the Drive/Head register
- Polls the Status register for `BSY` and `DRQ` bits before each 512-byte sector transfer
- Reads/writes 256 × `u16` words per sector via the data port
- `detect()` function checks if status port returns `0xFF` (floating bus = no disk)
- All ATA operations wrapped in `without_interrupts()` to prevent the scheduler from preempting mid-transfer

**Makefile additions:**
```makefile
$(BUILD_DIR)/disk.img:
    dd if=/dev/zero of=$(BUILD_DIR)/disk.img bs=1M count=10
    mkfs.fat -F 32 $(BUILD_DIR)/disk.img
    mcopy hello.txt ::hello.txt
    mcopy readme.txt ::readme.txt
```
QEMU flag: `-hda $(BUILD_DIR)/disk.img -boot order=a`

---

### 6.2 — FAT32 Filesystem (Written from Scratch)

**Problem:** The `fatfs` crate (and its `core_io` dependency) is incompatible with modern nightly Rust in a `no_std` environment.

**Solution:** Wrote a **complete FAT32 reader from scratch** — no external crate dependencies.

**File:** `kernel/src/fs/mod.rs`

**Implementation covers:**
| Component | Details |
|-----------|---------|
| BPB parsing | `#[repr(C, packed)]` struct over the 512-byte boot sector; extracts `bytes_per_sec`, `sec_per_clus`, `fat_start_sec`, `data_start_sec`, `root_cluster` |
| FAT chain traversal | Reads 4-byte FAT entries from the FAT table sector; follows `next_cluster()` links; terminates at `>= 0x0FFFFFF8` |
| Directory listing | `read_chain()` on the root cluster; parses `DirEntry` structs (32 bytes each); skips deleted (`0xE5`), LFN (`0x0F`), and Volume ID entries |
| 8.3 filename | `parse_83_name()` trims space-padded name and extension, joins with `.` |
| File reading | Case-insensitive 8.3 name match; reads full cluster chain; truncates to `file_size` bytes |

---

### 6.3 — Ring 3 User Mode

**Problem:** All code ran in Ring 0 (kernel mode) — any bug could corrupt the entire system. User programs must run in Ring 3 (unprivileged mode).

**Solution:** Added Ring 3 GDT segments and implemented an `iretq`-based privilege drop.

**Changes to `kernel/src/gdt.rs`:**
- Added `Descriptor::user_data_segment()` (selector `0x1B`) and `Descriptor::user_code_segment()` (selector `0x23`) to the GDT
- Updated `SYSCALL_KERNEL_STACK`: 16KB static buffer whose top address is stored in `TSS.privilege_stack_table[0]` — this is where the CPU switches to on syscall entry
- Used `core::ptr::addr_of!` to comply with Rust 2024's `static_mut_refs` lint

**`kernel/src/userspace/mod.rs`:**
Builds a fake interrupt frame on the stack:
```
push SS  (user data selector | 3)
push RSP (user stack top)
push 0x202 (RFLAGS, interrupts enabled)
push CS  (user code selector | 3)
push RIP (entry point)
iretq
```
The `iretq` instruction atomically loads all five values and drops to Ring 3.

---

### 6.4 — System Calls (SYSCALL/SYSRET)

**Problem:** User-mode code at Ring 3 cannot directly call kernel functions or access I/O ports — it needs a controlled, hardware-mediated gateway.

**Solution:** Configured the `SYSCALL`/`SYSRET` MSRs and wrote a naked-function dispatch table.

**File:** `kernel/src/syscall/mod.rs`

**MSR configuration:**
| MSR | Value | Purpose |
|-----|-------|---------|
| `IA32_EFER` (0xC0000080) | bit 0 set | Enable System Call Extensions |
| `IA32_STAR` (0xC0000081) | `0x0018_0008_0000_0000` | Kernel CS=0x08, SYSRET SS=0x18, CS=0x20 |
| `IA32_LSTAR` (0xC0000082) | `&syscall_handler` | Entry point address |
| `IA32_FMASK` (0xC0000084) | `0x200` | Clear IF flag on entry |

**Naked function handler:** Uses `#[unsafe(naked)]` with `naked_asm!` to save `RCX/R11/RBP`, call the Rust `dispatch()` function, and return with `sysretq`.

**Implemented syscalls:**
| Number | Name | Action |
|--------|------|--------|
| 1 | `sys_write` | Write UTF-8 bytes to VGA (fd=1) |
| 60 | `sys_exit` | Halt CPU in loop |

---

### 6.5 — The Shell

**Problem:** The OS had no user interface. All interaction was via serial debug prints.

**Solution:** Built a fully interactive kernel shell that reads keyboard input and dispatches commands.

**File:** `kernel/src/shell/mod.rs`

**Shell boot banner:**
```
  +-----------------------------------------+
  |        MYNEWOS Shell  v0.1              |
  |  Type 'help' for a list of commands.    |
  +-----------------------------------------+
```

**Commands implemented:**
| Command | Description |
|---------|-------------|
| `help` | Lists all available commands |
| `ls` | Lists files in the FAT32 root directory via ATA driver |
| `cat <file>` | Reads and prints a file from disk |
| `run <file>` | Loads a flat binary from FAT32 disk and executes it in Ring 3 |
| `uname` | Prints OS name, version, and architecture |
| `clear` | Scrolls the VGA buffer |

**Keyboard polling:** The shell's `read_line()` directly polls the `SCANCODE_QUEUE` (the same lock-free queue used by the async keyboard task) with a local `pc_keyboard` decoder, calling `enable_and_hlt()` between polls to yield to the scheduler while waiting for input.

---

## Summary

| Milestone | Status |
|-----------|--------|
| Async/Await Executor | ✅ |
| Preemptive Timer Scheduler | ✅ |
| Raw x86_64 Context Switch (ASM) | ✅ |
| APIC / IOAPIC Initialization | ✅ |
| ACPI Table Parsing | ✅ |
| SMP: 2-Core Boot via INIT/SIPI | ✅ |
| ATA PIO Disk Driver | ✅ |
| FAT32 Parser (from scratch) | ✅ |
| Ring 3 User Mode (iretq) | ✅ |
| SYSCALL/SYSRET MSR Setup | ✅ |
| Interactive Shell | ✅ |

**Total new files:** 14  
**Crates added:** `crossbeam-queue`, `conquer-once`, `futures-util`, `acpi`  
**Lines of code written:** ~1,800 (Rust + Assembly)
