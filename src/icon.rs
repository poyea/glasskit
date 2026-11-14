// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The picture of an icon, for a tray or a window.

use std::ffi::c_void;

use windows::Win32::Foundation::TRUE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, GetSystemMetrics, HICON, ICONINFO, SM_CXSMICON,
};

use crate::error::{Call, Error, Result};

/// The picture of a tray icon, or of a window with [`Window::set_icon`].
///
/// [`Window::set_icon`]: crate::window::Window::set_icon
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    size: u32,
    pixels: Vec<u32>,
}

impl Icon {
    /// The size in pixels that Windows uses for tray icons now. It changes with the display
    /// scale, so make the icon at this size just before [`Tray::new`].
    ///
    /// [`Tray::new`]: crate::tray::Tray::new
    pub fn tray_size() -> u32 {
        unsafe { GetSystemMetrics(SM_CXSMICON).max(16) as u32 }
    }

    /// A square icon of `size` by `size` pixels. `pixels` go row by row from the top-left
    /// corner. Each pixel is `0xAARRGGBB` with straight (not premultiplied) alpha.
    pub fn from_argb(size: u32, pixels: Vec<u32>) -> Result<Self> {
        if pixels.len() as u64 != u64::from(size) * u64::from(size) {
            return Err(Error::InvalidIcon {
                size,
                pixels: pixels.len(),
            });
        }
        Ok(Self { size, pixels })
    }

    /// The width and height in pixels.
    pub fn size(&self) -> u32 {
        self.size
    }
}

/// An icon handle that Windows destroys when the value is dropped.
pub(crate) struct IconHandle(pub(crate) HICON);

impl IconHandle {
    pub(crate) fn new(icon: &Icon) -> Result<Self> {
        create_icon(icon).map(Self)
    }
}

impl Drop for IconHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

fn create_icon(image: &Icon) -> Result<HICON> {
    let size = image.size as i32;
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .call("CreateDIBSection")?;
        std::slice::from_raw_parts_mut(bits as *mut u32, image.pixels.len())
            .copy_from_slice(&image.pixels);

        let stride = ((size + 15) / 16 * 2) as usize;
        let mask_bits = vec![0u8; stride * size as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon.call("CreateIconIndirect")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_needs_one_pixel_per_cell() {
        assert!(Icon::from_argb(2, vec![0; 4]).is_ok());
        assert!(matches!(
            Icon::from_argb(2, vec![0; 3]),
            Err(Error::InvalidIcon { size: 2, pixels: 3 })
        ));
    }
}
