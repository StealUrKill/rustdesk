// Standalone proof-of-concept: capture a single top-level window to a PNG
// using GDI PrintWindow / BitBlt. Intentionally self-contained — it touches
// no existing source files and adds no dependencies (winapi `winuser`/`wingdi`
// and the `repng` dev-dependency are already used by this crate).
//
// Goal: validate whether PrintWindow(PW_RENDERFULLCONTENT) / BitBlt cleanly
// captures a single window's contents (the "Select single window" feature),
// including the known limitation that some GPU/DWM-composited windows render
// black under a plain window-DC BitBlt.
//
// Usage (from libs/scrap):
//   cargo run --example capture_window                 # focus a window; captured after 3s
//   cargo run --example capture_window -- "Calculator" # capture the window with this title
//
// Produces two files so the methods can be compared:
//   window_bitblt.png      - BitBlt of the window DC
//   window_printwindow.png - PrintWindow(PW_RENDERFULLCONTENT)

#[cfg(windows)]
fn main() {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::{mem, ptr, thread, time::Duration};

    use winapi::shared::windef::{HBITMAP, HGDIOBJ, HWND, RECT};
    use winapi::um::wingdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use winapi::um::winuser::{
        FindWindowW, GetForegroundWindow, GetWindowDC, GetWindowRect, PrintWindow, ReleaseDC,
    };

    // Not exposed as a named constant by winapi 0.3.
    const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;

    // 1) Resolve the target window (by title, or the foreground window after a delay).
    let hwnd: HWND = match std::env::args().nth(1) {
        Some(title) => {
            let wide: Vec<u16> = OsStr::new(&title).encode_wide().chain(Some(0)).collect();
            let h = unsafe { FindWindowW(ptr::null(), wide.as_ptr()) };
            if h.is_null() {
                eprintln!("No window found with title {:?}", title);
                return;
            }
            h
        }
        None => {
            println!("Focus the window you want to capture; grabbing the foreground window in 3s...");
            thread::sleep(Duration::from_secs(3));
            unsafe { GetForegroundWindow() }
        }
    };
    if hwnd.is_null() {
        eprintln!("No target window.");
        return;
    }

    // 2) Window size from its rect (physical pixels; process is per-monitor DPI aware).
    let mut rect: RECT = unsafe { mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        eprintln!("GetWindowRect failed.");
        return;
    }
    let w = (rect.right - rect.left).max(1);
    let h = (rect.bottom - rect.top).max(1);
    println!("Target window: {}x{} at ({},{})", w, h, rect.left, rect.top);

    // 3) Capture with each method and save.
    unsafe {
        capture(hwnd, w, h, false, "window_bitblt.png");
        capture(hwnd, w, h, true, "window_printwindow.png");
    }

    unsafe fn capture(hwnd: HWND, w: i32, h: i32, use_print_window: bool, out: &str) {
        let hdc_window = GetWindowDC(hwnd);
        let hdc_mem = CreateCompatibleDC(hdc_window);
        let hbmp: HBITMAP = CreateCompatibleBitmap(hdc_window, w, h);
        let old = SelectObject(hdc_mem, hbmp as HGDIOBJ);

        let ok = if use_print_window {
            PrintWindow(hwnd, hdc_mem, PW_RENDERFULLCONTENT) != 0
        } else {
            BitBlt(hdc_mem, 0, 0, w, h, hdc_window, 0, 0, SRCCOPY) != 0
        };
        if !ok {
            eprintln!("{}: capture call returned failure", out);
        }

        // Pull the bitmap bits out as 32-bit top-down BGRA.
        let mut bmi: BITMAPINFO = mem::zeroed();
        bmi.bmiHeader.biSize = mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // negative height => top-down rows
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );
        if lines == 0 {
            eprintln!("{}: GetDIBits failed", out);
        }

        // BGRA -> RGBA for repng.
        let mut rgba = Vec::with_capacity(buf.len());
        for px in buf.chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        match File::create(out) {
            Ok(f) => {
                repng::encode(f, w as u32, h as u32, &rgba).unwrap();
                println!("Saved {}", out);
            }
            Err(e) => eprintln!("create {} failed: {}", out, e),
        }

        // Cleanup.
        SelectObject(hdc_mem, old);
        DeleteObject(hbmp as HGDIOBJ);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd, hdc_window);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This example is Windows-only.");
}
