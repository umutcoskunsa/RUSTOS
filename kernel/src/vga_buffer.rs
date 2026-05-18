use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;
use alloc::vec::Vec;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0, Blue = 1, Green = 2, Cyan = 3, Red = 4, Magenta = 5, Brown = 6, LightGray = 7,
    DarkGray = 8, LightBlue = 9, LightGreen = 10, LightCyan = 11, LightRed = 12, Pink = 13, Yellow = 14, White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
    pub fn get_foreground(&self) -> Color { unsafe { core::mem::transmute(self.0 & 0x0F) } }
    pub fn get_background(&self) -> Color { unsafe { core::mem::transmute((self.0 & 0xF0) >> 4) } }
}

fn vga_to_rgb(color: Color) -> u32 {
    match color {
        Color::Black => 0x000000, Color::Blue => 0x0000AA, Color::Green => 0x00AA00, Color::Cyan => 0x00AAAA,
        Color::Red => 0xAA0000, Color::Magenta => 0xAA00AA, Color::Brown => 0xAA5500, Color::LightGray => 0xAAAAAA,
        Color::DarkGray => 0x555555, Color::LightBlue => 0x5555FF, Color::LightGreen => 0x55FF55, Color::LightCyan => 0x55FFFF,
        Color::LightRed => 0xFF5555, Color::Pink => 0xFF55FF, Color::Yellow => 0xFFFF55, Color::White => 0xFFFFFF,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

pub struct Writer {
    column_position: usize,
    row_position:    usize,
    color_code:      ColorCode,
    width:           usize,
    height:          usize,
    buffer:          Option<Vec<ScreenChar>>,
    legacy_buffer:   Option<&'static mut [Volatile<ScreenChar>]>,
}

impl Writer {
    pub fn init_graphics(&mut self, width_px: u32, height_px: u32) {
        let cols = (width_px / 8) as usize;
        let rows = (height_px / 8) as usize;
        self.width = cols;
        self.height = rows;
        // This is safe because heap is initialized by now
        self.buffer = Some(alloc::vec![ScreenChar { ascii_character: b' ', color_code: self.color_code }; cols * rows]);
        self.column_position = 0;
        self.row_position = 0;
        self.clear();
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            b'\x08' => {
                if self.column_position > 0 {
                    self.column_position -= 1;
                    let sc = ScreenChar { ascii_character: b' ', color_code: self.color_code };
                    self.set_char(self.row_position, self.column_position, sc);
                }
            }
            byte => {
                if self.column_position >= self.width {
                    self.new_line();
                }
                let sc = ScreenChar { ascii_character: byte, color_code: self.color_code };
                self.set_char(self.row_position, self.column_position, sc);
                self.column_position += 1;
            }
        }
    }

    fn set_char(&mut self, row: usize, col: usize, sc: ScreenChar) {
        if row >= self.height || col >= self.width { return; }
        
        // Update logical buffer if exists
        if let Some(ref mut buf) = self.buffer {
            buf[row * self.width + col] = sc;
        }
        
        // Update physical display
        if crate::graphics::is_active() {
            let fg = vga_to_rgb(sc.color_code.get_foreground());
            let bg = vga_to_rgb(sc.color_code.get_background());
            crate::graphics::draw_char((col * 8) as u32, (row * 8) as u32, sc.ascii_character as char, fg, bg);
        } else if let Some(ref mut legacy) = self.legacy_buffer {
            if row < 25 && col < 80 {
                legacy[row * 80 + col].write(sc);
            }
        }
    }

    fn new_line(&mut self) {
        if self.row_position < self.height - 1 {
            self.row_position += 1;
            self.column_position = 0;
        } else {
            // Scroll
            for r in 1..self.height {
                for c in 0..self.width {
                    let sc = if let Some(ref buf) = self.buffer {
                        buf[r * self.width + c]
                    } else if let Some(ref legacy) = self.legacy_buffer {
                        if r < 25 && c < 80 { legacy[r * 80 + c].read() }
                        else { ScreenChar { ascii_character: b' ', color_code: self.color_code } }
                    } else {
                        ScreenChar { ascii_character: b' ', color_code: self.color_code }
                    };
                    self.set_char(r - 1, c, sc);
                }
            }
            self.clear_row(self.height - 1);
            self.column_position = 0;
        }
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar { ascii_character: b' ', color_code: self.color_code };
        for col in 0..self.width {
            self.set_char(row, col, blank);
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' | b'\x08' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn clear(&mut self) {
        let blank = ScreenChar { ascii_character: b' ', color_code: self.color_code };
        for r in 0..self.height {
            for c in 0..self.width {
                self.set_char(r, c, blank);
            }
        }
        self.column_position = 0;
        self.row_position = 0;
    }

    pub fn write_raw_at(&mut self, row: usize, col: usize, byte: u8, color: ColorCode) {
        self.set_char(row, col, ScreenChar { ascii_character: byte, color_code: color });
    }

    pub fn write_str_at(&mut self, row: usize, col: usize, s: &str, color: ColorCode) {
        let mut c = col;
        for byte in s.bytes() {
            if c >= self.width { break; }
            let b = match byte { 0x20..=0x7e => byte, _ => b'?' };
            self.set_char(row, c, ScreenChar { ascii_character: b, color_code: color });
            c += 1;
        }
    }

    pub fn get_width(&self) -> usize { self.width }
    pub fn get_height(&self) -> usize { self.height }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result { self.write_string(s); Ok(()) }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        row_position:    0,
        color_code:      ColorCode::new(Color::LightGreen, Color::Black),
        width:           80,
        height:          25,
        buffer:          None,
        legacy_buffer:   Some(unsafe { core::slice::from_raw_parts_mut(0xb8000 as *mut Volatile<ScreenChar>, 80 * 25) }),
    });
}

#[macro_export]
macro_rules! print { ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*))); }

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
        crate::serial::SERIAL1.lock().write_fmt(args).unwrap_or(());
    });
}
