use alloc::string::String;
use alloc::vec::Vec;

/// Run the interactive kernel shell on the VGA console.
pub fn run() -> ! {
    crate::println!("");
    crate::println!("  +-----------------------------------------+");
    crate::println!("  |        MYNEWOS Shell  v0.1              |");
    crate::println!("  |  Type 'help' for a list of commands.    |");
    crate::println!("  +-----------------------------------------+");
    crate::println!("");

    let mut input = String::new();
    loop {
        crate::print!("> ");
        input.clear();
        read_line(&mut input);
        let trimmed = input.trim();
        if trimmed.is_empty() { continue; }
        handle_command(trimmed);
    }
}

/// Block until a full line of input is collected from the keyboard.
fn read_line(buf: &mut String) {
    loop {
        // Poll the keyboard scancode queue via our existing async infrastructure
        if let Some(c) = poll_char() {
            match c {
                '\n' | '\r' => {
                    crate::println!("");
                    return;
                }
                '\x08' => {
                    // Backspace / Delete: erase last typed character
                    if !buf.is_empty() {
                        buf.pop();
                        crate::print!("\x08"); // VGA now handles this natively
                    }
                }
                c => {
                    buf.push(c);
                    crate::print!("{}", c);
                }
            }
        }
        // Yield the CPU briefly to avoid starving other kernel work
        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

/// Try to pop one decoded character from the keyboard scancode queue or the serial port.
fn poll_char() -> Option<char> {
    use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
    use spin::Mutex;
    use lazy_static::lazy_static;

    // --- 1. Check Serial Port (so user can type in the terminal directly) ---
    {
        // To read from COM1: port 0x3F8 offsets. Line Status Register is +5.
        let status = unsafe { x86_64::instructions::port::PortReadOnly::<u8>::new(0x3F8 + 5).read() };
        if (status & 1) != 0 {
            // Data Ready! Read it from 0x3F8.
            let data = unsafe { x86_64::instructions::port::PortReadOnly::<u8>::new(0x3F8).read() };
            return match data {
                0x7F | 0x08 => Some('\x08'), // Backspace / Delete
                0x0D => Some('\n'),          // Carriage Return
                c    => Some(c as char),     // Standard ASCII
            };
        }
    }

    // --- 2. Check PS/2 Keyboard Scancode Queue ---
    lazy_static! {
        static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(
            Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore)
        );
    }

    let queue = crate::task::keyboard::SCANCODE_QUEUE.try_get().ok()?;
    let scancode = queue.pop()?;
    let mut kb = KB.lock();
    if let Ok(Some(event)) = kb.add_byte(scancode) {
        if let Some(key) = kb.process_keyevent(event) {
            match key {
                DecodedKey::Unicode(c) => return Some(c),
                // Map Delete and Backspace raw keys to \x08
                DecodedKey::RawKey(pc_keyboard::KeyCode::Backspace) => return Some('\x08'),
                DecodedKey::RawKey(pc_keyboard::KeyCode::Delete)    => return Some('\x08'),
                DecodedKey::RawKey(_) => {}
            }
        }
    }
    None
}

/// Dispatch a shell command.
fn handle_command(cmd: &str) {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    match parts[0] {
        "help" => cmd_help(),
        "clear" => cmd_clear(),
        "ls"   => cmd_ls(),
        "cat"  => {
            if parts.len() < 2 {
                crate::println!("Usage: cat <filename>");
            } else {
                cmd_cat(parts[1].trim());
            }
        }
        "cap"  => {
            if parts.len() < 2 {
                crate::println!("Usage: cap <filename>");
            } else {
                crate::cap::open(parts[1].trim());
                // After editor exits, redraw the shell prompt
                crate::println!("(returned from cap)");
            }
        }
        "run" => {
            if parts.len() < 2 {
                crate::println!("Usage: run <filename>");
            } else {
                cmd_run(parts[1].trim());
            }
        }
        "ps" => cmd_ps(),
        "kill" => {
            if parts.len() < 2 {
                crate::println!("Usage: kill <pid>");
            } else {
                cmd_kill(parts[1].trim());
            }
        }
        "uname" => crate::println!("MYNEWOS 0.1.0 (x86_64) - Built with Rust 🦀"),
        other  => crate::println!("Unknown command: '{}'. Type 'help'.", other),
    }
}

fn cmd_help() {
    crate::println!("Available commands:");
    crate::println!("  help         - Show this help message");
    crate::println!("  ls           - List files on the FAT32 disk");
    crate::println!("  cat <file>   - Print contents of a file");
    crate::println!("  cap <file>   - Open file in the cap text editor");
    crate::println!("  run <file>   - Execute an ELF binary in background");
    crate::println!("  ps           - List all background processes");
    crate::println!("  kill <pid>   - Terminate a background process");
    crate::println!("  uname        - Print OS information");
    crate::println!("  clear        - Clear the screen");
}

fn cmd_clear() {
    // Overwrite the VGA buffer with spaces
    for _ in 0..25 {
        crate::println!("");
    }
}

fn cmd_ls() {
    if !crate::disk::detect() {
        crate::println!("Error: No ATA disk detected.");
        return;
    }
    let entries = crate::fs::list_root();
    if entries.is_empty() {
        crate::println!("(empty or unreadable filesystem)");
    } else {
        for e in &entries {
            crate::println!("  {}", e);
        }
    }
}

fn cmd_cat(filename: &str) {
    if !crate::disk::detect() {
        crate::println!("Error: No ATA disk detected.");
        return;
    }
    match crate::fs::read_file(filename) {
        Some(data) => {
            if let Ok(s) = core::str::from_utf8(&data) {
                crate::println!("{}", s.trim_end());
            } else {
                crate::println!("(binary file, {} bytes)", data.len());
            }
        }
        None => crate::println!("Error: file '{}' not found.", filename),
    }
}

fn cmd_run(filename: &str) {
    if !crate::disk::detect() {
        crate::println!("Error: No ATA disk detected.");
        return;
    }

    // Reject obvious non-executables by extension
    let lower = filename.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    if ext == "txt" || ext == "md" || ext == "rs" || ext == "cfg" {
        crate::println!("Error: '{}' is a text file, not an executable.", filename);
        return;
    }

    match crate::fs::read_file(filename) {
        Some(data) => {
            crate::println!("Loading '{}' ({} bytes)...", filename, data.len());

            if crate::elf::is_elf(&data) {
                // --- ELF binary: use the process spawner ---
                match crate::process::spawn(&data, filename) {
                    Ok(pid) => {
                        crate::println!("Process {} ({}) scheduled for Execution!", pid, filename);
                    }
                    Err(e) => crate::println!("Failed to spawn ELF: {}", e),
                }
            } else {
                crate::println!("Error: Legacy flat binaries are no longer supported. Please compile as ELF!");
            }
        }
        None => crate::println!("Error: file '{}' not found.", filename),
    }
}

fn cmd_ps() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let table = crate::process::PROCESS_TABLE.lock();
        crate::println!("PID  | NAME           | STATE      | CR3");
        crate::println!("-----|----------------|------------|-----------");
        for p in table.iter() {
            let state_str = match p.state {
                crate::process::ProcessState::Ready      => "Ready",
                crate::process::ProcessState::Running    => "Running",
                crate::process::ProcessState::Zombie(_)  => "Zombie", 
            };
            crate::println!("{:<4} | {:<14} | {:<10} | {:#x}", 
                p.pid, p.name, state_str, p.cr3);
        }
    });
}

fn cmd_kill(pid_str: &str) {
    if let Ok(pid) = pid_str.parse::<usize>() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            match crate::process::kill(pid) {
                Ok(_) => crate::println!("Process {} killed.", pid),
                Err(e) => crate::println!("Error: {}", e),
            }
        });
    } else {
        crate::println!("Invalid PID: {}", pid_str);
    }
}
