/// cap - the MYNEWOS text editor
/// Controls: Arrow keys to move | Backspace/Delete to erase | Enter for newline
///           Ctrl+S (F2) to save | Esc to quit without saving
use alloc::string::String;
use alloc::vec::Vec;
use crate::vga_buffer::{Color, ColorCode, WRITER};

// Pre-baked color codes helper
fn color(fg: Color, bg: Color) -> ColorCode { ColorCode::new(fg, bg) }

struct Editor {
    filename: String,
    lines:    Vec<Vec<u8>>,       // file content as lines of bytes
    cur_row:  usize,              // cursor row in file (0-based)
    cur_col:  usize,              // cursor col in current line (0-based)
    scroll:   usize,              // first visible line index
    dirty:    bool,               // unsaved changes?
    width:    usize,
    height:   usize,
}

impl Editor {
    fn new(filename: &str) -> Self {
        let (w, h) = {
            let writer = WRITER.lock();
            (writer.get_width(), writer.get_height())
        };

        let content = crate::fs::read_file(filename)
            .unwrap_or_else(Vec::new);

        let mut lines: Vec<Vec<u8>> = Vec::new();
        let mut current: Vec<u8> = Vec::new();
        for &b in &content {
            if b == b'\n' {
                lines.push(current.clone());
                current.clear();
            } else if b == b'\t' {
                current.push(b);
            } else if b >= 32 && b <= 126 {
                current.push(b);
            }
        }
        lines.push(current);
        if lines.is_empty() { lines.push(Vec::new()); }

        Editor {
            filename: String::from(filename),
            lines,
            cur_row: 0,
            cur_col: 0,
            scroll:  0,
            dirty:   false,
            width:   w,
            height:  h,
        }
    }

    fn render(&self) {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut w = WRITER.lock();
            let edit_rows = self.height - 2;

            // --- Header bar (row 0) ---
            let hdr_color = color(Color::Black, Color::Cyan);
            let hdr = alloc::format!(
                " CAP  |  {}{}  |  {}",
                self.filename,
                if self.dirty { " [modified]" } else { "" },
                "F2/Ctrl+S: Save   Esc: Quit"
            );
            let hdr_padded = format_padded(&hdr, self.width);
            w.write_str_at(0, 0, &hdr_padded, hdr_color);

            // --- Content rows ---
            let normal  = color(Color::LightGray, Color::Black);
            let cursor_color = color(Color::Black, Color::LightGray);
            let lineno  = color(Color::DarkGray,  Color::Black);

            for screen_row in 0..edit_rows {
                let file_row = self.scroll + screen_row;
                let vga_row  = screen_row + 1; // offset for header

                // Clear the row first
                w.write_str_at(vga_row, 0, &" ".repeat(self.width), normal);

                if file_row >= self.lines.len() { continue; }

                // Line number (4 digits + space)
                let lnum = alloc::format!("{:3} ", file_row + 1);
                w.write_str_at(vga_row, 0, &lnum, lineno);

                // Line content
                let content_start_col = 4;
                let max_chars = self.width - content_start_col;
                let line = &self.lines[file_row];

                for (i, &b) in line.iter().enumerate().take(max_chars) {
                    let col = content_start_col + i;
                    let is_cursor = file_row == self.cur_row && i == self.cur_col;
                    let ch_color  = if is_cursor { cursor_color } else { normal };
                    let display   = if b >= 0x20 && b <= 0x7e { b } else { b'?' };
                    w.write_raw_at(vga_row, col, display, ch_color);
                }

                // Cursor at end of line
                if file_row == self.cur_row && self.cur_col == line.len() && self.cur_col < max_chars {
                    w.write_raw_at(vga_row, content_start_col + line.len(), b' ', cursor_color);
                }
            }

            // --- Status bar ---
            let st_color = color(Color::White, Color::DarkGray);
            let status = alloc::format!(
                " Ln {}/{}  Col {}  {} chars",
                self.cur_row + 1,
                self.lines.len(),
                self.cur_col + 1,
                self.lines.iter().map(|l| l.len()).sum::<usize>() + self.lines.len().saturating_sub(1),
            );
            let st_padded = format_padded(&status, self.width);
            w.write_str_at(self.height - 1, 0, &st_padded, st_color);
        });
    }

    fn save(&mut self) -> bool {
        let mut bytes: Vec<u8> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            bytes.extend_from_slice(line);
            if i + 1 < self.lines.len() { bytes.push(b'\n'); }
        }
        let ok = crate::fs::write_file(&self.filename, &bytes);
        if ok {
            self.dirty = false;
            // Note: In a real app we'd have a status() method, 
            // for now let's just use the dirty flag logic
        }
        ok
    }

    fn adjust_scroll(&mut self) {
        let edit_rows = self.height - 2;
        if self.cur_row < self.scroll {
            self.scroll = self.cur_row;
        } else if self.cur_row >= self.scroll + edit_rows {
            self.scroll = self.cur_row - edit_rows + 1;
        }
    }
}

// ---- Public entry point ----

pub fn open(filename: &str) {
    // Validate extension - allow anything for text editing
    let mut state = Editor::new(filename);
    state.render();

    let mut ctrl_held = false;

    loop {
        let key = poll_key();
        match key {
            EdKey::None => {
                x86_64::instructions::interrupts::enable_and_hlt();
                continue;
            }
            EdKey::CtrlDown  => { ctrl_held = true;  continue; }
            EdKey::CtrlUp    => { ctrl_held = false; continue; }
            EdKey::Escape    => break, // quit without saving
            EdKey::Save      => { save_file(&mut state); }
            EdKey::Char('s') | EdKey::Char('S') if ctrl_held => { save_file(&mut state); }
            EdKey::Enter => {
                // Split current line at cursor
                let rest = state.lines[state.cur_row].split_off(state.cur_col);
                state.lines.insert(state.cur_row + 1, rest);
                state.cur_row += 1;
                state.cur_col  = 0;
                state.dirty    = true;
            }
            EdKey::Backspace => {
                if state.cur_col > 0 {
                    state.cur_col -= 1;
                    state.lines[state.cur_row].remove(state.cur_col);
                    state.dirty = true;
                } else if state.cur_row > 0 {
                    // Merge with previous line
                    let cur_line = state.lines.remove(state.cur_row);
                    state.cur_row -= 1;
                    state.cur_col  = state.lines[state.cur_row].len();
                    state.lines[state.cur_row].extend_from_slice(&cur_line);
                    state.dirty = true;
                }
            }
            EdKey::Delete => {
                let line_len = state.lines[state.cur_row].len();
                if state.cur_col < line_len {
                    state.lines[state.cur_row].remove(state.cur_col);
                    state.dirty = true;
                } else if state.cur_row + 1 < state.lines.len() {
                    let next = state.lines.remove(state.cur_row + 1);
                    state.lines[state.cur_row].extend_from_slice(&next);
                    state.dirty = true;
                }
            }
            EdKey::ArrowLeft => {
                if state.cur_col > 0 {
                    state.cur_col -= 1;
                } else if state.cur_row > 0 {
                    state.cur_row -= 1;
                    state.cur_col  = state.lines[state.cur_row].len();
                }
            }
            EdKey::ArrowRight => {
                let line_len = state.lines[state.cur_row].len();
                if state.cur_col < line_len {
                    state.cur_col += 1;
                } else if state.cur_row + 1 < state.lines.len() {
                    state.cur_row += 1;
                    state.cur_col  = 0;
                }
            }
            EdKey::ArrowUp => {
                if state.cur_row > 0 {
                    state.cur_row -= 1;
                    state.cur_col  = state.cur_col.min(state.lines[state.cur_row].len());
                }
            }
            EdKey::ArrowDown => {
                if state.cur_row + 1 < state.lines.len() {
                    state.cur_row += 1;
                    state.cur_col  = state.cur_col.min(state.lines[state.cur_row].len());
                }
            }
            EdKey::Home => { state.cur_col = 0; }
            EdKey::End  => { state.cur_col = state.lines[state.cur_row].len(); }
            EdKey::Char(c) if c.is_ascii() && (c >= ' ' || c == '\t') => {
                state.lines[state.cur_row].insert(state.cur_col, c as u8);
                state.cur_col += 1;
                state.dirty    = true;
            }
            _ => { continue; }
        }

        state.adjust_scroll();
        state.render();
    }

    // On exit: clear screen and redraw the shell
    x86_64::instructions::interrupts::without_interrupts(|| WRITER.lock().clear());
}

fn save_file(state: &mut Editor) {
    if state.save() {
        // Show save confirmation briefly in status bar
        let ok_color = ColorCode::new(Color::Black, Color::Green);
        let filename = state.filename.clone();
        let width = state.width;
        let height = state.height;
        x86_64::instructions::interrupts::without_interrupts(|| {
            let msg = format_padded(&alloc::format!(" Saved: {}", filename), width);
            WRITER.lock().write_str_at(height - 1, 0, &msg, ok_color);
        });
        for _ in 0..5_000_000 { core::hint::spin_loop(); }
    } else {
        let err_color = ColorCode::new(Color::White, Color::Red);
        let width = state.width;
        let height = state.height;
        x86_64::instructions::interrupts::without_interrupts(|| {
            let msg = format_padded(" ERROR: Could not save file!", width);
            WRITER.lock().write_str_at(height - 1, 0, &msg, err_color);
        });
        for _ in 0..5_000_000 { core::hint::spin_loop(); }
    }
    state.render();
}

fn format_padded(s: &str, width: usize) -> String {
    let mut out = String::from(s);
    while out.len() < width { out.push(' '); }
    out.truncate(width);
    out
}

// ---- Keyboard abstraction for the editor ----

#[derive(Debug, Clone, Copy)]
enum EdKey {
    None,
    Char(char),
    Enter,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Save,    // F2
    Escape,
    CtrlDown,
    CtrlUp,
}

fn poll_key() -> EdKey {
    use pc_keyboard::{layouts, HandleControl, Keyboard, KeyEvent, KeyCode, KeyState, ScancodeSet1, DecodedKey};
    use spin::Mutex;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(
            Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::MapLettersToUnicode)
        );
    }

    let queue = match crate::task::keyboard::SCANCODE_QUEUE.try_get() {
        Ok(q) => q,
        Err(_) => return EdKey::None,
    };
    let scancode = match queue.pop() {
        Some(s) => s,
        None    => return EdKey::None,
    };

    let mut kb = KB.lock();
    if let Ok(Some(event)) = kb.add_byte(scancode) {
        let KeyEvent { code, state } = event;

        // Track Ctrl key state
        if code == KeyCode::LControl || code == KeyCode::RControl {
            return if state == KeyState::Down { EdKey::CtrlDown } else { EdKey::CtrlUp };
        }

        if state != KeyState::Down { return EdKey::None; }

        // Map raw keys first (these don't produce Unicode)
        match code {
            KeyCode::ArrowUp    => return EdKey::ArrowUp,
            KeyCode::ArrowDown  => return EdKey::ArrowDown,
            KeyCode::ArrowLeft  => return EdKey::ArrowLeft,
            KeyCode::ArrowRight => return EdKey::ArrowRight,
            KeyCode::Home       => return EdKey::Home,
            KeyCode::End        => return EdKey::End,
            KeyCode::Delete     => return EdKey::Delete,
            KeyCode::Backspace  => return EdKey::Backspace,
            KeyCode::Escape     => return EdKey::Escape,
            KeyCode::F2         => return EdKey::Save,
            _                   => {}
        }

        // Try to get a Unicode character
        if let Some(key) = kb.process_keyevent(KeyEvent { code, state }) {
            match key {
                DecodedKey::Unicode('\n') | DecodedKey::Unicode('\r') => return EdKey::Enter,
                DecodedKey::Unicode('\x08')                           => return EdKey::Backspace,
                DecodedKey::Unicode('\x1B')                           => return EdKey::Escape,
                DecodedKey::Unicode(c)                                => return EdKey::Char(c),
                DecodedKey::RawKey(_)                                 => {}
            }
        }
    }
    EdKey::None
}
