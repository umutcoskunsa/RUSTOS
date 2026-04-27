/// User-space jump: transition from Ring 0 to Ring 3.

/// Top of the default user stack virtual address.
/// We put it in the upper half (PML4 index 1) so it doesn't collide with the
/// kernel's supervisor-only identity mappings in PML4 index 0.
pub const USER_STACK_VIRT: u64 = 0x0000_0080_8000_0000;
const USER_STACK_SIZE: usize   = 64 * 1024; // 64 KiB



