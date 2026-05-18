// =============================================================================
// graphics.rs — Framebuffer graphics driver for MYNEWOS
//
// The BIOS VBE Mode Info Block is stored at physical 0x7E00 by stage2.asm.
// Offsets (from VBE 3.0 spec):
//   0x00  attributes (u16)
//   0x12  bytes_per_scan_line (u16)
//   0x12  Xres (u16)    -- actually offset 0x12
//   0x14  Yres (u16)
//   0x1B  bits_per_pixel (u8)
//   0x28  framebuffer physical address (u32)
// =============================================================================

use core::sync::atomic::{AtomicBool, Ordering};

/// Physical address where stage2 stores the VBE Mode Info Block.
const VBE_INFO_PHYS: u64 = 0x7E00;

/// Global framebuffer state
static mut FB_ADDR: u64 = 0;
static mut FB_WIDTH: u32 = 0;
static mut FB_HEIGHT: u32 = 0;
static mut FB_PITCH: u32 = 0; // bytes per row
static mut FB_BPP: u8 = 0;
static GRAPHICS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    unsafe {
        let info = VBE_INFO_PHYS as *const u8;

        let xres      = u16::from_le_bytes([*info.add(0x12), *info.add(0x13)]) as u32;
        let yres      = u16::from_le_bytes([*info.add(0x14), *info.add(0x15)]) as u32;
        let bpp       = *info.add(0x19);
        let pitch     = u16::from_le_bytes([*info.add(0x10), *info.add(0x11)]) as u32;
        let fb_phys   = u32::from_le_bytes([
            *info.add(0x28), *info.add(0x29), *info.add(0x2A), *info.add(0x2B)
        ]) as u64;

        if fb_phys == 0 || xres == 0 || yres == 0 {
            crate::serial_println!("GFX: No VESA framebuffer found (still in text mode).");
            return;
        }

        FB_ADDR   = fb_phys;
        FB_WIDTH  = xres;
        FB_HEIGHT = yres;
        FB_PITCH  = pitch;
        FB_BPP    = bpp;

        // Identity-map the entire framebuffer so we can write to it
        let fb_size = (pitch as u64) * (yres as u64);
        crate::memory::map_identity_region(fb_phys, fb_phys + fb_size + 0x1000);

        GRAPHICS_ENABLED.store(true, Ordering::Relaxed);

        crate::serial_println!(
            "GFX: Framebuffer at {:#x}, {}x{} {}bpp, pitch={}",
            fb_phys, xres, yres, bpp, pitch
        );
    }
}

/// Returns true if the graphics engine is active.
pub fn is_active() -> bool {
    GRAPHICS_ENABLED.load(Ordering::Relaxed)
}

/// Width of the screen in pixels.
pub fn width() -> u32  { unsafe { FB_WIDTH } }
/// Height of the screen in pixels.
pub fn height() -> u32 { unsafe { FB_HEIGHT } }

/// Write a single pixel. Color is 0x00RRGGBB.
#[inline(always)]
pub fn put_pixel(x: u32, y: u32, color: u32) {
    if !is_active() { return; }
    unsafe {
        if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
        let offset = (y * FB_PITCH + x * (FB_BPP as u32 / 8)) as u64;
        let pixel_ptr = (FB_ADDR + offset) as *mut u32;
        pixel_ptr.write_volatile(color);
    }
}

/// Fill a rectangle with a solid color.
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    let x_end = (x + w).min(width());
    let y_end = (y + h).min(height());
    for row in y..y_end {
        for col in x..x_end {
            put_pixel(col, row, color);
        }
    }
}

/// Clear the entire screen to a given color.
pub fn clear(color: u32) {
    if !is_active() { return; }
    unsafe {
        let pitch = FB_PITCH as usize;
        let height = FB_HEIGHT as usize;
        let base = FB_ADDR as *mut u8;
        for row in 0..height {
            let row_ptr = base.add(row * pitch) as *mut u32;
            let cols = FB_WIDTH as usize;
            for col in 0..cols {
                row_ptr.add(col).write_volatile(color);
            }
        }
    }
}

/// Draw a simple 8x8 bitmap character from a packed u64 font.
/// Each bit in the u64 represents one pixel (MSB = top-left).
pub fn draw_char(x: u32, y: u32, ch: char, fg: u32, bg: u32) {
    let bitmap = FONT_8X8[ch as usize % 128];
    for row in 0..8u32 {
        for col in 0..8u32 {
            // MSB of u64 = row 0. Within each row byte, LSB = leftmost pixel (col 0).
            let bit = (bitmap >> (56 - row * 8 + col)) & 1;
            put_pixel(x + col, y + row, if bit == 1 { fg } else { bg });
        }
    }
}

/// Draw a null-terminated string on screen.
pub fn draw_str(x: u32, y: u32, s: &str, fg: u32, bg: u32) {
    let mut cx = x;
    for ch in s.chars() {
        if ch == '\n' {
            return; // caller handles newlines
        }
        draw_char(cx, y, ch, fg, bg);
        cx += 8;
    }
}

// =============================================================================
// Minimal 8x8 pixel font (ASCII 32-127) stored as packed 64-bit bitmaps.
// Each u64 = 8 rows x 8 cols, MSB = pixel (0,0), LSB = pixel (7,7).
// This is a hand-packed excerpt of the classic IBM CP437 / VGA ROM font.
// =============================================================================
#[rustfmt::skip]
static FONT_8X8: [u64; 128] = [
    // 0x00 - 0x1F  (control chars - blank)
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
    // 0x20 ' '
    0x0000000000000000,
    // 0x21 '!'
    0x183C3C1818001800,
    // 0x22 '"'
    0x3636000000000000,
    // 0x23 '#'
    0x36367F367F363600,
    // 0x24 '$'
    0x0C3E031E301F0C00,
    // 0x25 '%'
    0x006333180C666300,
    // 0x26 '&'
    0x1C361C6E3B336E00,
    // 0x27 '''
    0x0606030000000000,
    // 0x28 '('
    0x180C0606060C1800,
    // 0x29 ')'
    0x060C181818060C00, // Note: swapped intentionally for ')' shape
    // 0x2A '*'
    0x00663CFF3C660000,
    // 0x2B '+'
    0x000C0C3F0C0C0000,
    // 0x2C ','
    0x00000000000C0C06,
    // 0x2D '-'
    0x0000003F00000000,
    // 0x2E '.'
    0x00000000000C0C00,
    // 0x2F '/'
    0x6030180C06030100,
    // 0x30 '0'
    0x3E63737B6F673E00,
    // 0x31 '1'
    0x0C0E0C0C0C0C3F00,
    // 0x32 '2'
    0x1E33301C06333F00,
    // 0x33 '3'
    0x1E33301C30331E00,
    // 0x34 '4'
    0x383C36337F307800,
    // 0x35 '5'
    0x3F031F3030331E00,
    // 0x36 '6'
    0x1C06031F33331E00,
    // 0x37 '7'
    0x3F3330180C0C0C00,
    // 0x38 '8'
    0x1E33331E33331E00,
    // 0x39 '9'
    0x1E33333E30180E00,
    // 0x3A ':'
    0x000C0C00000C0C00,
    // 0x3B ';'
    0x000C0C00000C0C06,
    // 0x3C '<'
    0x180C0603060C1800,
    // 0x3D '='
    0x00003F00003F0000,
    // 0x3E '>'
    0x060C1830180C0600,
    // 0x3F '?'
    0x1E3330180C000C00,
    // 0x40 '@'
    0x3E637B7B7B031E00,
    // 0x41 'A'
    0x0C1E33333F333300,
    // 0x42 'B'
    0x3F66663E66663F00,
    // 0x43 'C'
    0x3C66030303663C00,
    // 0x44 'D'
    0x1F36666666361F00,
    // 0x45 'E'
    0x7F46161E16467F00,
    // 0x46 'F'
    0x7F46161E16060F00,
    // 0x47 'G'
    0x3C66030373667C00,
    // 0x48 'H'
    0x3333333F33333300,
    // 0x49 'I'
    0x1E0C0C0C0C0C1E00,
    // 0x4A 'J'
    0x7830303033331E00,
    // 0x4B 'K'
    0x6766361E36666700,
    // 0x4C 'L'
    0x0F06060646667F00,
    // 0x4D 'M'
    0x63777F7F6B636300,
    // 0x4E 'N'
    0x63676F7B73636300,
    // 0x4F 'O'
    0x1C36636363361C00,
    // 0x50 'P'
    0x3F66663E06060F00,
    // 0x51 'Q'
    0x1E33333B1E387000,
    // 0x52 'R'
    0x3F66663E36666700,
    // 0x53 'S'
    0x1E33071C38331E00,
    // 0x54 'T'
    0x3F2D0C0C0C0C1E00,
    // 0x55 'U'
    0x3333333333333F00,
    // 0x56 'V'
    0x33333333331E0C00,
    // 0x57 'W'
    0x6363636B7F776300,
    // 0x58 'X'
    0x6363361C1C366300,
    // 0x59 'Y'
    0x3333331E0C0C1E00,
    // 0x5A 'Z'
    0x7F6331180C677F00,
    // 0x5B '['
    0x1E06060606061E00,
    // 0x5C '\'
    0x03060C1830604000,
    // 0x5D ']'
    0x1E18181818181E00,
    // 0x5E '^'
    0x081C366300000000,
    // 0x5F '_'
    0x00000000000000FF,
    // 0x60 '`'
    0x0C0C180000000000,
    // 0x61 'a'
    0x00001E303E336E00,
    // 0x62 'b'
    0x0706063E66663B00,
    // 0x63 'c'
    0x00001E3303331E00,
    // 0x64 'd'
    0x3830303E33336E00,
    // 0x65 'e'
    0x00001E333F031E00,
    // 0x66 'f'
    0x1C36060F0606060F,
    // 0x67 'g'
    0x00006E33333E301F,
    // 0x68 'h'
    0x0706366E66666700,
    // 0x69 'i'
    0x0C001E0C0C0C1E00,
    // 0x6A 'j'
    0x300030303033331E,
    // 0x6B 'k'
    0x07066676361E3600,
    // 0x6C 'l'
    0x0E0C0C0C0C0C1E00,
    // 0x6D 'm'
    0x00003B7F6B6B6300,
    // 0x6E 'n'
    0x00001F3333333300,
    // 0x6F 'o'
    0x00001E3333331E00,
    // 0x70 'p'
    0x00003B66663E060F,
    // 0x71 'q'
    0x00006E33333E3078,
    // 0x72 'r'
    0x00003B6E66060F00,
    // 0x73 's'
    0x00003E031E301F00,
    // 0x74 't'
    0x080C3E0C0C2C1800,
    // 0x75 'u'
    0x00003333333E0000,
    // 0x76 'v'
    0x00003333331E0C00,
    // 0x77 'w'
    0x0000636B7F7F3600,
    // 0x78 'x'
    0x000063361C366300,
    // 0x79 'y'
    0x00003333333E301F,
    // 0x7A 'z'
    0x00003F190C263F00,
    // 0x7B '{'
    0x380C0C070C0C3800,
    // 0x7C '|'
    0x1818180018181800,
    // 0x7D '}'
    0x070C0C380C0C0700,
    // 0x7E '~'
    0x6E3B000000000000,
    // 0x7F DEL
    0x0000000000000000,
];
