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

    cmd_ls(""); // Show files on startup
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
                0x0D => Some('\n'),          // Carriage Return (Map to \n)
                0x0A => None,                // Ignore Line Feed (avoid double \n)
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
    let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
    if parts.is_empty() { return; }
    match parts[0] {
        "help" => cmd_help(),
        "clear" => cmd_clear(),
        "ls"   => {
            let path = if parts.len() > 1 { parts[1].trim() } else { "" };
            cmd_ls(path);
        }
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
        "nettest" => {
            // Send a UDP packet to QEMU router (10.0.2.2)
            let dest_mac = [0x52, 0x55, 0x0A, 0x00, 0x02, 0x02];
            let dest_ip = [10, 0, 2, 2];
            let msg = b"Hello from MYNEWOS over UDP!";
            crate::net::udp::send(dest_mac, dest_ip, 8080, 1234, msg);
            crate::println!("Sent UDP Packet to 10.0.2.2:8080!");
        }
        "gfxtest" => {
            if !crate::graphics::is_active() {
                crate::println!("GFX: No framebuffer (still in text mode).");
            } else {
                let w = crate::graphics::width();
                let h = crate::graphics::height();
                // Paint a gradient background
                for y in 0..h {
                    for x in 0..w {
                        let r = (x * 255 / w) as u32;
                        let b = (y * 255 / h) as u32;
                        crate::graphics::put_pixel(x, y, (r << 16) | b);
                    }
                }
                // Draw a white banner
                crate::graphics::fill_rect(0, 0, w, 20, 0x00FFFFFF);
                crate::graphics::draw_str(8, 6, "MYNEWOS Graphics Engine - Hello!", 0x00000000, 0x00FFFFFF);
                crate::println!("GFX: Painted {}x{} framebuffer!", w, h);
            }
        }
        "rm" => {
            if parts.len() < 2 {
                crate::println!("Usage: rm <filename>");
            } else {
                if crate::fs::delete_file(parts[1].trim()) {
                    crate::println!("File deleted.");
                } else {
                    crate::println!("Error: Could not delete file.");
                }
            }
        }
        "mv" => {
            if parts.len() < 3 {
                crate::println!("Usage: mv <old> <new>");
            } else {
                if crate::fs::rename_file(parts[1].trim(), parts[2].trim()) {
                    crate::println!("File moved/renamed.");
                } else {
                    crate::println!("Error: Could not move file.");
                }
            }
        }
        "mkdir" => {
            if parts.len() < 2 {
                crate::println!("Usage: mkdir <dirname>");
            } else {
                if crate::fs::create_dir(parts[1].trim()) {
                    crate::println!("Directory created.");
                } else {
                    crate::println!("Error: Could not create directory.");
                }
            }
        }
        "ln" => {
            if parts.len() < 4 || parts[1].trim() != "-s" {
                crate::println!("Usage: ln -s <target> <link_name>");
            } else {
                let target = parts[2].trim();
                let link_name = parts[3].trim();
                if crate::fs::create_symlink(link_name, target) {
                    crate::println!("Symlink created.");
                } else {
                    crate::println!("Error: Could not create symlink.");
                }
            }
        }
        "mkfs.ext2" => {
            if parts.len() < 2 {
                crate::println!("Usage: mkfs.ext2 <drive_id> [start_lba] [num_sectors]");
                crate::println!("Example: mkfs.ext2 1 2048 20480");
            } else {
                let drive_id = parts[1].trim().parse::<u8>().unwrap_or(1);
                let start_lba = if parts.len() > 2 { parts[2].trim().parse::<u32>().unwrap_or(0) } else { 0 };
                let num_sectors = if parts.len() > 3 { parts[3].trim().parse::<u32>().unwrap_or(20480) } else { 20480 };
                
                crate::println!("Formatting drive {} at LBA {}...", drive_id, start_lba);
                if crate::fs::ext2::Ext2Fs::format(drive_id, start_lba, num_sectors) {
                    crate::println!("Format complete.");
                } else {
                    crate::println!("Error: Format failed.");
                }
            }
        }
        "chmod" => {
            if parts.len() < 3 {
                crate::println!("Usage: chmod <mode_octal> <path>");
            } else {
                let mode = u16::from_str_radix(parts[1].trim(), 8).unwrap_or(0o644);
                if crate::fs::chmod(parts[2].trim(), mode) {
                    crate::println!("Permissions updated.");
                } else {
                    crate::println!("Error: chmod failed.");
                }
            }
        }
        "chown" => {
            if parts.len() < 4 {
                crate::println!("Usage: chown <uid> <gid> <path>");
            } else {
                let uid = parts[1].trim().parse::<u16>().unwrap_or(0);
                let gid = parts[2].trim().parse::<u16>().unwrap_or(0);
                if crate::fs::chown(parts[3].trim(), uid, gid) {
                    crate::println!("Ownership updated.");
                } else {
                    crate::println!("Error: chown failed.");
                }
            }
        }
        "su" => {
            if parts.len() < 2 {
                crate::println!("Usage: su <uid>");
            } else {
                let uid = parts[1].trim().parse::<u16>().unwrap_or(0);
                *crate::fs::CURRENT_UID.lock() = uid;
                crate::println!("Switched to UID {}.", uid);
            }
        }
        "whoami" => {
            let uid = *crate::fs::CURRENT_UID.lock();
            let gid = *crate::fs::CURRENT_GID.lock();
            crate::println!("UID: {}, GID: {}", uid, gid);
        }
        "uname" => crate::println!("MYNEWOS 0.1.0 (x86_64) - Built with Rust "),
        other  => crate::println!("Unknown command: '{}'. Type 'help'.", other),
    }
}

fn cmd_help() {
    crate::println!("Available commands:");
    crate::println!("  help            - Show this help message");
    crate::println!("  ls              - List files on the FAT32 disk");
    crate::println!("  cat <file>      - Print contents of a file");
    crate::println!("  cap <file>      - Open file in the cap text editor");
    crate::println!("  run <file>      - Execute an ELF binary in background");
    crate::println!("  ps              - List all background processes");
    crate::println!("  kill <pid>      - Terminate a background process");
    crate::println!("  uname           - Print OS information");
    crate::println!("  clear           - Clear the screen");
    crate::println!("  chmod <mode> <p>- Change file permissions (octal)");
    crate::println!("  chown <u:g> <p> - Change file owner/group");
    crate::println!("  su <uid>        - Switch current user");
    crate::println!("  whoami          - Print current user identity");
    crate::println!("  nettest         - Send a UDP packet to 10.0.2.2:8080");
    crate::println!("  gfxtest         - Paint a test gradient on the framebuffer");
}

fn cmd_clear() {
    // Overwrite the VGA buffer with spaces
    for _ in 0..25 {
        crate::println!("");
    }
}

fn cmd_ls(path: &str) {
    if !crate::disk::detect(0) {
        crate::println!("Error: No ATA disk detected.");
        return;
    }
    let target_path = if path.is_empty() { "/" } else { path };
    let entries = crate::fs::list_dir(target_path);
    if entries.is_empty() {
        crate::println!("(empty or unreadable directory: {})", target_path);
    } else {
        for e in &entries {
            crate::println!("  {}", e);
        }
    }
}

fn cmd_cat(filename: &str) {
    if !crate::disk::detect(0) {
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
    if !crate::disk::detect(0) {
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
                crate::process::ProcessState::Blocked    => "Blocked",
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
