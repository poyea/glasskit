// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

use std::fmt;

use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeMedianCut, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};

use crate::error::{Call, Error, Result};

/// The largest width or height of an image, in pixels.
const MAX_SIDE: u32 = 4096;

/// A picture for a [`Prompt`](super::Prompt). The popup shows one pixel of the image as one
/// pixel at 100% display scale, and makes it smaller to fit the popup.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub(super) width: u32,
    pub(super) height: u32,
    /// Premultiplied BGRA, 4 bytes for each pixel, row after row.
    pub(super) pixels: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Image {
    /// An image from pixels in `0xAARRGGBB` form, row after row from the top left. The alpha
    /// is straight, not premultiplied.
    ///
    /// It fails with [`Error::InvalidImage`] if the number of pixels is not
    /// `width * height`, or if a side is 0 or larger than 4096.
    pub fn from_argb(width: u32, height: u32, argb: &[u32]) -> Result<Self> {
        check(width, height, argb.len())?;
        let mut pixels = Vec::with_capacity(argb.len() * 4);
        for &p in argb {
            let a = p >> 24;
            let premultiply = |c: u32| ((c * a + 127) / 255) as u8;
            pixels.extend_from_slice(&[
                premultiply(p & 0xFF),
                premultiply((p >> 8) & 0xFF),
                premultiply((p >> 16) & 0xFF),
                a as u8,
            ]);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// An image from the bytes of a file in a format that Windows reads, such as PNG, JPEG,
    /// GIF (the first frame), BMP, or TIFF.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        // The calls are balanced: COM stays as it was for the thread.
        let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let image = decode(bytes);
        if com.is_ok() {
            unsafe { CoUninitialize() };
        }
        image
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
}

fn check(width: u32, height: u32, pixels: usize) -> Result<()> {
    let valid = (1..=MAX_SIDE).contains(&width)
        && (1..=MAX_SIDE).contains(&height)
        && pixels == (width * height) as usize;
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidImage {
            width,
            height,
            pixels,
        })
    }
}

fn decode(bytes: &[u8]) -> Result<Image> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .call("CoCreateInstance(WICImagingFactory)")?;
        let stream = factory.CreateStream().call("CreateStream")?;
        stream
            .InitializeFromMemory(bytes)
            .call("InitializeFromMemory")?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .call("CreateDecoderFromStream")?;
        let frame = decoder.GetFrame(0).call("GetFrame")?;
        let converter = factory
            .CreateFormatConverter()
            .call("CreateFormatConverter")?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeMedianCut,
            )
            .call("Initialize")?;
        let (mut width, mut height) = (0, 0);
        converter.GetSize(&mut width, &mut height).call("GetSize")?;
        check(width, height, (width * height) as usize)?;
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        converter
            .CopyPixels(std::ptr::null(), width * 4, &mut pixels)
            .call("CopyPixels")?;
        Ok(Image {
            width,
            height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 x 1 PNG: one opaque red pixel and one transparent pixel.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0xF4,
        0x22, 0x7F, 0x8A, 0x00, 0x00, 0x00, 0x0F, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x44, 0x0C, 0x0C, 0x00, 0x0C, 0xFC, 0x01, 0xFF, 0xE5, 0x86, 0xA8, 0xD2,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn argb_becomes_premultiplied_bgra() {
        let image = Image::from_argb(2, 1, &[0xFF11_2233, 0x80FF_0000]).unwrap();
        assert_eq!(image.pixels, [0x33, 0x22, 0x11, 0xFF, 0, 0, 0x80, 0x80]);
    }

    #[test]
    fn a_wrong_pixel_count_is_an_error() {
        assert!(matches!(
            Image::from_argb(2, 2, &[0; 3]),
            Err(Error::InvalidImage { pixels: 3, .. })
        ));
        assert!(Image::from_argb(0, 0, &[]).is_err());
    }

    #[test]
    fn decodes_a_png() {
        let image = Image::decode(PNG).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(&image.pixels[..4], [0, 0, 0xFF, 0xFF]);
        assert_eq!(image.pixels[7], 0);
    }

    #[test]
    fn bytes_that_are_not_an_image_are_an_error() {
        assert!(Image::decode(b"not an image").is_err());
    }
}
