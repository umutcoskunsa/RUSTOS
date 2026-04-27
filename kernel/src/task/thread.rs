use alloc::alloc::{alloc, Layout};
use core::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ThreadId(u64);

impl ThreadId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        ThreadId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Represents the x86_64 CPU state pushed onto the stack during an interrupt.
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ThreadContext {
    // General Purpose Registers (pushed manually by assembly)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9:  u64,
    pub r8:  u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,

    // Interrupt Frame (pushed by hardware on interrupt)
    pub rip: u64,
    pub cs:  u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss:  u64,
}

pub struct Thread {
    pub id: ThreadId,
    pub stack_ptr: usize,
    // Store the stack allocation so it's not leaked if the thread is destroyed
    #[allow(dead_code)]
    pub stack_bottom: usize,
}

const STACK_SIZE: usize = 8192; // 8 KiB

impl Thread {
    pub fn new(entry_point: extern "C" fn() -> !) -> Self {
        // Allocate a stack dynamically on the heap
        let layout = Layout::from_size_align(STACK_SIZE, 16).unwrap();
        let stack_bottom = unsafe { alloc(layout) } as usize;
        let stack_top = stack_bottom + STACK_SIZE;

        // Ensure alignment
        let stack_top = stack_top & !0xF;

        let mut thread = Thread {
            id: ThreadId::new(),
            stack_ptr: stack_top - core::mem::size_of::<ThreadContext>(),
            stack_bottom,
        };

        let context = unsafe { &mut *(thread.stack_ptr as *mut ThreadContext) };
        context.r15 = 0;
        context.r14 = 0;
        context.r13 = 0;
        context.r12 = 0;
        context.r11 = 0;
        context.r10 = 0;
        context.r9  = 0;
        context.r8  = 0;
        context.rbp = 0;
        context.rdi = 0;
        context.rsi = 0;
        context.rdx = 0;
        context.rcx = 0;
        context.rbx = 0;
        context.rax = 0;

        context.rip = entry_point as u64;
        context.cs  = 0x08; // 64-bit Code Segment
        context.rflags = 0x202; // IF flag enabled
        context.rsp = stack_top as u64;
        context.ss  = 0x10; // 64-bit Data Segment

        thread
    }
}
