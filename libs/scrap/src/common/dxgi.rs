#[cfg(feature = "vram")]
use crate::AdapterDevice;
use crate::{common::TraitCapturer, dxgi, Frame, Pixfmt};
use std::{
    io::{
        self,
        ErrorKind::{NotFound, TimedOut, WouldBlock},
    },
    time::Duration,
};

pub struct Capturer {
    inner: dxgi::Capturer,
    width: usize,
    height: usize,
}

impl Capturer {
    pub fn new(display: Display) -> io::Result<Capturer> {
        let width = display.width();
        let height = display.height();
        let inner = dxgi::Capturer::new(display.0)?;
        Ok(Capturer {
            inner,
            width,
            height,
        })
    }

    pub fn cancel_gdi(&mut self) {
        self.inner.cancel_gdi()
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }
}

impl TraitCapturer for Capturer {
    fn frame<'a>(&'a mut self, timeout: Duration) -> io::Result<Frame<'a>> {
        match self.inner.frame(timeout.as_millis() as _) {
            Ok(frame) => Ok(frame),
            Err(ref error) if error.kind() == TimedOut => Err(WouldBlock.into()),
            Err(error) => Err(error),
        }
    }

    fn is_gdi(&self) -> bool {
        self.inner.is_gdi()
    }

    fn set_gdi(&mut self) -> bool {
        self.inner.set_gdi()
    }

    #[cfg(feature = "vram")]
    fn device(&self) -> AdapterDevice {
        self.inner.device()
    }

    #[cfg(feature = "vram")]
    fn set_output_texture(&mut self, texture: bool) {
        self.inner.set_output_texture(texture);
    }
}

pub struct PixelBuffer<'a> {
    data: &'a [u8],
    pixfmt: Pixfmt,
    width: usize,
    height: usize,
    stride: Vec<usize>,
}

impl<'a> PixelBuffer<'a> {
    pub fn new(data: &'a [u8], pixfmt: Pixfmt, width: usize, height: usize) -> Self {
        let stride0 = data.len() / height;
        let mut stride = Vec::new();
        stride.push(stride0);
        PixelBuffer {
            data,
            pixfmt,
            width,
            height,
            stride,
        }
    }

    #[allow(non_snake_case)]
    pub fn with_BGRA(data: &'a [u8], width: usize, height: usize) -> Self {
        Self::new(data, Pixfmt::BGRA, width, height)
    }
}

impl<'a> crate::TraitPixelBuffer for PixelBuffer<'a> {
    fn data(&self) -> &[u8] {
        self.data
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn stride(&self) -> Vec<usize> {
        self.stride.clone()
    }

    fn pixfmt(&self) -> Pixfmt {
        self.pixfmt
    }
}

pub struct WindowCapturer {
    hwnd: winapi::shared::windef::HWND,
    width: usize,
    height: usize,
    data: Vec<u8>,
    old: Vec<u8>,
    hdc_mem: winapi::shared::windef::HDC,
    hbmp: winapi::shared::windef::HBITMAP,
    prev_bmp: winapi::shared::windef::HGDIOBJ,
}

impl WindowCapturer {
    pub fn new(hwnd: isize, width: usize, height: usize) -> Self {
        use winapi::um::wingdi::{CreateCompatibleBitmap, CreateCompatibleDC, SelectObject};
        use winapi::um::winuser::{GetDC, ReleaseDC};
        let hwnd = hwnd as winapi::shared::windef::HWND;
        let (hdc_mem, hbmp, prev_bmp) = unsafe {
            let hdc_screen = GetDC(std::ptr::null_mut());
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let hbmp = CreateCompatibleBitmap(hdc_screen, width as i32, height as i32);
            let prev_bmp = SelectObject(hdc_mem, hbmp as _);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            (hdc_mem, hbmp, prev_bmp)
        };
        WindowCapturer {
            hwnd,
            width,
            height,
            data: vec![0u8; width * height * 4],
            old: Vec::new(),
            hdc_mem,
            hbmp,
            prev_bmp,
        }
    }
}

impl Drop for WindowCapturer {
    fn drop(&mut self) {
        use winapi::um::wingdi::{DeleteDC, DeleteObject, SelectObject};
        unsafe {
            SelectObject(self.hdc_mem, self.prev_bmp);
            DeleteObject(self.hbmp as _);
            DeleteDC(self.hdc_mem);
        }
    }
}

impl crate::TraitCapturer for WindowCapturer {
    fn frame<'a>(
        &'a mut self,
        _timeout: std::time::Duration,
    ) -> std::io::Result<crate::Frame<'a>> {
        use winapi::um::wingdi::{
            GetDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        use winapi::um::winuser::PrintWindow;
        const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;
        let w = self.width as i32;
        let h = self.height as i32;
        unsafe {
            if PrintWindow(self.hwnd, self.hdc_mem, PW_RENDERFULLCONTENT) == 0 {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            bmi.bmiHeader.biHeight = -h; // negative => top-down rows (BGRA)
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;
            if GetDIBits(
                self.hdc_mem,
                self.hbmp,
                0,
                h as u32,
                self.data.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            ) == 0
            {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
        }
        super::would_block_if_equal(&mut self.old, &self.data)?;
        Ok(crate::Frame::PixelBuffer(PixelBuffer::with_BGRA(
            &self.data,
            self.width,
            self.height,
        )))
    }

    fn is_gdi(&self) -> bool {
        true
    }

    fn set_gdi(&mut self) -> bool {
        true
    }

    #[cfg(feature = "vram")]
    fn device(&self) -> crate::AdapterDevice {
        crate::AdapterDevice::default()
    }

    #[cfg(feature = "vram")]
    fn set_output_texture(&mut self, _texture: bool) {}
}

pub struct Display(dxgi::Display);

impl Display {
    pub fn primary() -> io::Result<Display> {
        // not implemented yet
        Err(NotFound.into())
    }

    pub fn all() -> io::Result<Vec<Display>> {
        let displays_gdi = dxgi::Displays::get_from_gdi()
            .drain(..)
            .map(Display)
            .collect::<Vec<_>>();

        let displays_dxgi = Self::all_().unwrap_or(Default::default());

        // Return gdi displays if dxgi is not supported
        if displays_dxgi.is_empty() {
            println!("Display got from gdi");
            return Ok(displays_gdi);
        }

        // Return dxgi displays if length is not equal
        if displays_dxgi.len() != displays_gdi.len() {
            return Ok(displays_dxgi);
        }

        // Check if names are equal
        let names_gdi = displays_gdi.iter().map(|d| d.name()).collect::<Vec<_>>();
        let names_dxgi = displays_dxgi.iter().map(|d| d.name()).collect::<Vec<_>>();
        for name in names_gdi.iter() {
            if !names_dxgi.contains(name) {
                return Ok(displays_dxgi);
            }
        }

        // Reorder displays from dxgi
        let mut displays_dxgi = displays_dxgi;
        let mut displays_dxgi_ordered = Vec::new();
        for name in names_gdi.iter() {
            let pos = match displays_dxgi.iter().position(|d| d.name() == *name) {
                Some(pos) => pos,
                None => {
                    // unreachable!
                    0
                }
            };
            displays_dxgi_ordered.push(displays_dxgi.remove(pos));
        }

        Ok(displays_dxgi_ordered)
    }

    fn all_() -> io::Result<Vec<Display>> {
        Ok(dxgi::Displays::new()?.map(Display).collect::<Vec<_>>())
    }

    pub fn width(&self) -> usize {
        self.0.width() as usize
    }

    pub fn height(&self) -> usize {
        self.0.height() as usize
    }

    pub fn name(&self) -> String {
        use std::ffi::OsString;
        use std::os::windows::prelude::*;
        OsString::from_wide(self.0.name())
            .to_string_lossy()
            .to_string()
    }

    pub fn is_online(&self) -> bool {
        self.0.is_online()
    }

    pub fn origin(&self) -> (i32, i32) {
        self.0.origin()
    }

    pub fn is_primary(&self) -> bool {
        // https://docs.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-devmodea
        self.origin() == (0, 0)
    }

    #[cfg(feature = "vram")]
    pub fn adapter_luid(&self) -> Option<i64> {
        self.0.adapter_luid()
    }
}

pub struct CapturerMag {
    inner: dxgi::mag::CapturerMag,
    data: Vec<u8>,
}

impl CapturerMag {
    pub fn is_supported() -> bool {
        dxgi::mag::CapturerMag::is_supported()
    }

    pub fn new(origin: (i32, i32), width: usize, height: usize) -> io::Result<Self> {
        Ok(CapturerMag {
            inner: dxgi::mag::CapturerMag::new(origin, width, height)?,
            data: Vec::new(),
        })
    }

    pub fn exclude(&mut self, cls: &str, name: &str) -> io::Result<bool> {
        self.inner.exclude(cls, name)
    }
    // ((x, y), w, h)
    pub fn get_rect(&self) -> ((i32, i32), usize, usize) {
        self.inner.get_rect()
    }
}

impl TraitCapturer for CapturerMag {
    fn frame<'a>(&'a mut self, _timeout_ms: Duration) -> io::Result<Frame<'a>> {
        self.inner.frame(&mut self.data)?;
        Ok(Frame::PixelBuffer(PixelBuffer::with_BGRA(
            &self.data,
            self.inner.get_rect().1,
            self.inner.get_rect().2,
        )))
    }

    fn is_gdi(&self) -> bool {
        false
    }

    fn set_gdi(&mut self) -> bool {
        false
    }

    #[cfg(feature = "vram")]
    fn device(&self) -> AdapterDevice {
        AdapterDevice::default()
    }

    #[cfg(feature = "vram")]
    fn set_output_texture(&mut self, _texture: bool) {}
}
