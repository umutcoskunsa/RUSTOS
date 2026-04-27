use super::thread::Thread;
use alloc::collections::VecDeque;
use spin::Mutex;
use lazy_static::lazy_static;

pub struct Scheduler {
    ready_queue: VecDeque<Thread>,
    current_thread: Option<Thread>,
}

lazy_static! {
    pub static ref SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            ready_queue: VecDeque::new(),
            current_thread: None,
        }
    }

    pub fn spawn(&mut self, thread: Thread) {
        self.ready_queue.push_back(thread);
    }

    pub fn schedule(&mut self, old_rsp: usize) -> usize {
        // If there are no threads waiting, just return current stack
        if self.ready_queue.is_empty() {
            return old_rsp;
        }

        // Save current thread state
        let prev_thread = match self.current_thread.take() {
            Some(mut thread) => {
                thread.stack_ptr = old_rsp;
                thread
            }
            None => {
                // This is the first switch: we completely hijacked the boot thread!
                Thread {
                    id: super::thread::ThreadId::new(),
                    stack_ptr: old_rsp,
                    stack_bottom: 0, 
                }
            }
        };

        // Put the previous thread at the back of the queue (Round Robin)
        self.ready_queue.push_back(prev_thread);

        // Pop the next thread
        let next_thread = self.ready_queue.pop_front().unwrap();

        // Overwrite the RSP reference to the new thread's stack pointer!
        let new_rsp = next_thread.stack_ptr;
        self.current_thread = Some(next_thread);
        new_rsp
    }
}

/// Legacy scheduler removed from assembly bindings
pub fn schedule_timer(old_rsp: usize) -> usize {
    crate::interrupts::TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    let new_rsp = SCHEDULER.lock().schedule(old_rsp);

    // After scheduling, acknowledge the hardware interrupt!
    crate::apic::end_of_interrupt();

    new_rsp
}
