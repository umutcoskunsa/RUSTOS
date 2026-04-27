use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct BumpAllocator {
    heap_start: AtomicUsize,
    heap_end: AtomicUsize,
    next: AtomicUsize,
}

impl BumpAllocator {
    pub const fn empty() -> Self {
        BumpAllocator {
            heap_start: AtomicUsize::new(0),
            heap_end: AtomicUsize::new(0),
            next: AtomicUsize::new(0),
        }
    }

    /// Initializes the allocator with a continuous buffer address range.
    pub unsafe fn init(&self, start: usize, size: usize) {
        self.heap_start.store(start, Ordering::SeqCst);
        self.heap_end.store(start + size, Ordering::SeqCst);
        self.next.store(start, Ordering::SeqCst);
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        
        loop {
            let current = self.next.load(Ordering::Relaxed);
            let heap_end = self.heap_end.load(Ordering::Relaxed);

            // Align address
            let aligned = (current + align - 1) & !(align - 1);
            let next_addr = aligned + size;

            if next_addr > heap_end {
                return core::ptr::null_mut(); // Out of memory
            }

            if self.next.compare_exchange(
                current,
                next_addr,
                Ordering::SeqCst,
                Ordering::SeqCst
            ).is_ok() {
                return aligned as *mut u8;
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator never deallocates!
    }
}
