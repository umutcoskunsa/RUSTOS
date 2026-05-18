use alloc::sync::Arc;
use alloc::collections::VecDeque;
use spin::Mutex;

#[derive(Clone)]
pub enum FileDescriptor {
    StandardInput,
    StandardOutput,
    StandardError,
    PipeRead(Arc<Mutex<Pipe>>),
    PipeWrite(Arc<Mutex<Pipe>>),
    File(Arc<Mutex<FileState>>),
}

pub struct FileState {
    pub data: alloc::vec::Vec<u8>,
    pub offset: usize,
    pub name: alloc::string::String,
}

pub struct Pipe {
    pub buffer: VecDeque<u8>,
    pub max_size: usize,
    pub read_closed: bool,
    pub write_closed: bool,
}

impl Pipe {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_size),
            max_size,
            read_closed: false,
            write_closed: false,
        }
    }
}

pub fn create_pipe() -> (FileDescriptor, FileDescriptor) {
    let pipe = Arc::new(Mutex::new(Pipe::new(4096)));
    (
        FileDescriptor::PipeRead(pipe.clone()),
        FileDescriptor::PipeWrite(pipe)
    )
}
