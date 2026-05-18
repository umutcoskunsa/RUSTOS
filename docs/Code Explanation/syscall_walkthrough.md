# Syscall & SYSRET Walkthrough

This document explains the high-performance system call mechanism implemented in the kernel using the x86_64 `SYSCALL` and `SYSRET` instructions.

## 1. Hardware Initialization (`init`)
The `init` function configures the Model Specific Registers (MSRs) that the CPU uses when the `syscall` instruction is executed.

### IA32_EFER (Extended Feature Enable Register)
We enable the **SCE (System Call Extensions)** bit. Without this, the `syscall` instruction triggers an Invalid Opcode exception.

### IA32_STAR (The "Star" Register)
This is where we define the GDT selectors the CPU should switch to.
- **Bits 47:32 (Kernel Base)**: Set to `0x08`. On `syscall`, the CPU sets `CS = 0x08` and `SS = 0x10`.
- **Bits 63:48 (User Base)**: Set to `0x10`. This was our "Bug Magnet."
  - Hardware uses `Base + 8` for User SS.
  - Hardware uses `Base + 16` for User CS.
  - By setting this to `0x10` (Kernel Data), we get:
    - **User SS = 0x18** (User Data)
    - **User CS = 0x20** (User Code)

### IA32_LSTAR (Long System Target Address)
We write the memory address of our `syscall_handler` function into this register. When the user executes `syscall`, the CPU immediately jumps to this address.

---

## 2. The Assembly Handler (`syscall_handler`)
Since `syscall` doesn't use the stack (to save time), we have to be very careful.

### The `swapgs` Dance
The user's `GS` register contains user-specific data. We use the `swapgs` instruction to swap it with the `IA32_KERNEL_GS_BASE` MSR, which contains our `PerCpu` pointer. This allows the kernel to find its own stack without a valid stack pointer.

### Stack Transition
1. Save the user's `RSP` into `PerCpu.user_rsp`.
2. Load the kernel's `RSP` from `PerCpu.kernel_stack`.
3. Align the stack to 16 bytes for Rust compatibility.

### Saving State
`syscall` overwrites `RCX` (with the return RIP) and `R11` (with the return RFLAGS). We push these onto the kernel stack so we can restore them later.

---

## 3. Returning to User Mode (`SYSRET`)
Returning is the reverse:
1. `swapgs` back to the user's GS.
2. Load the user's `RSP` back.
3. Use `sysretq` to jump back to the address in `RCX`.

> [!IMPORTANT]
> `SYSRET` does not check the stack. It relies entirely on the `STAR` MSR being correctly aligned with your GDT indices.
