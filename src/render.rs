// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! What all custom-drawn windows share: the GPU devices, a drawing surface, the glass
//! backdrop, rounded panels, and text sizes.

use crate::error::{Call, Result};
use crate::theme::rgba;
use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionBrush, CompositionDrawingSurface, CompositionGraphicsDevice, Compositor,
    SpriteVisual, VisualCollection,
};
use windows::Win32::Foundation::{POINT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Device,
    ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorInterop,
};
use windows::core::{BOOL, Interface, PCWSTR, w};
use windows_numerics::{Matrix3x2, Vector2};

/// The GPU devices. Keep them only while a window shows, because they use much memory.
pub struct Graphics {
    pub(crate) compositor: Compositor,
    pub(crate) graphics_device: CompositionGraphicsDevice,
    dwrite: IDWriteFactory,
    _d2d_device: ID2D1Device,
}

impl Drop for Graphics {
    fn drop(&mut self) {
        let _ = self.graphics_device.Close();
        let _ = self.compositor.Close();
    }
}

impl Graphics {
    pub fn new() -> Result<Self> {
        unsafe {
            let compositor = Compositor::new().call("creating Compositor")?;

            let d3d = create_d3d_device()?;
            let dxgi: IDXGIDevice = d3d.cast().call("QueryInterface(IDXGIDevice)")?;
            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)
                    .call("D2D1CreateFactory")?;
            let d2d_device = d2d_factory
                .CreateDevice(&dxgi)
                .call("ID2D1Factory1::CreateDevice")?;
            let graphics_device = compositor
                .cast::<ICompositorInterop>()
                .call("QueryInterface(ICompositorInterop)")?
                .CreateGraphicsDevice(&d2d_device)
                .call("CreateGraphicsDevice")?;

            let dwrite: IDWriteFactory =
                DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).call("DWriteCreateFactory")?;

            Ok(Self {
                compositor,
                graphics_device,
                dwrite,
                _d2d_device: d2d_device,
            })
        }
    }

    pub fn text_format(
        &self,
        size_px: f32,
        weight: DWRITE_FONT_WEIGHT,
        centered: bool,
    ) -> Result<IDWriteTextFormat> {
        self.format(w!("Segoe UI Variable Text"), size_px, weight, centered)
    }

    /// A centered format in the icon font of Windows. Windows 10 does not have Segoe Fluent
    /// Icons, and has the same icons in Segoe MDL2 Assets.
    pub fn icon_format(&self, size_px: f32) -> Result<IDWriteTextFormat> {
        self.format(
            icon_family(&self.dwrite),
            size_px,
            DWRITE_FONT_WEIGHT_NORMAL,
            true,
        )
    }

    fn format(
        &self,
        family: PCWSTR,
        size_px: f32,
        weight: DWRITE_FONT_WEIGHT,
        centered: bool,
    ) -> Result<IDWriteTextFormat> {
        unsafe {
            let format = self
                .dwrite
                .CreateTextFormat(
                    family,
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size_px,
                    w!(""),
                )
                .call("CreateTextFormat")?;
            if centered {
                format
                    .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
                    .call("SetTextAlignment")?;
                format
                    .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                    .call("SetParagraphAlignment")?;
            }
            Ok(format)
        }
    }

    pub fn dwrite(&self) -> IDWriteFactory {
        self.dwrite.clone()
    }

    pub fn text_layout(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        width: f32,
    ) -> Result<IDWriteTextLayout> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            self.dwrite
                .CreateTextLayout(&wide, format, width, f32::MAX)
                .call("CreateTextLayout")
        }
    }
}

pub fn layout_height(layout: &IDWriteTextLayout) -> Result<f32> {
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics).call("GetMetrics")? };
    Ok(metrics.height)
}

fn create_d3d_device() -> Result<ID3D11Device> {
    fn create(driver: D3D_DRIVER_TYPE) -> windows::core::Result<ID3D11Device> {
        let mut device = None;
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
        }
        Ok(device.expect("D3D11CreateDevice succeeded without a device"))
    }
    create(D3D_DRIVER_TYPE_HARDWARE)
        .or_else(|_| create(D3D_DRIVER_TYPE_WARP))
        .call("D3D11CreateDevice")
}

pub fn layout_width(layout: &IDWriteTextLayout) -> Result<f32> {
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics).call("GetMetrics")? };
    Ok(metrics.width)
}

/// The icon font of Windows. Windows 10 does not have Segoe Fluent Icons, and has the
/// same icons in Segoe MDL2 Assets.
pub(crate) fn icon_family(dwrite: &IDWriteFactory) -> PCWSTR {
    let fluent = w!("Segoe Fluent Icons");
    let mut fonts = None;
    let (mut index, mut found) = (0u32, BOOL(0));
    unsafe {
        let _ = dwrite.GetSystemFontCollection(&mut fonts, false);
        if let Some(fonts) = &fonts {
            let _ = fonts.FindFamilyName(fluent, &mut index, &mut found);
        }
    }
    if found.as_bool() {
        fluent
    } else {
        w!("Segoe MDL2 Assets")
    }
}

pub fn rect(left: f32, top: f32, width: f32, height: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// A drawing surface, and the sprite visual that shows it.
pub struct Surface {
    surface: CompositionDrawingSurface,
    sprite: SpriteVisual,
}

impl Surface {
    /// A surface of this many pixels.
    pub fn new(gfx: &Graphics, width: f32, height: f32) -> windows::core::Result<Self> {
        let size = Vector2 {
            X: width.max(1.0),
            Y: height.max(1.0),
        };
        let surface = gfx.graphics_device.CreateDrawingSurface(
            Size {
                Width: size.X,
                Height: size.Y,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )?;
        let sprite = gfx.compositor.CreateSpriteVisual()?;
        sprite.SetSize(size)?;
        sprite.SetBrush(&gfx.compositor.CreateSurfaceBrushWithSurface(&surface)?)?;
        Ok(Self { surface, sprite })
    }

    pub fn sprite(&self) -> &SpriteVisual {
        &self.sprite
    }

    /// Make the surface this many pixels.
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        let (w, h) = (width.max(1), height.max(1));
        let interop: ICompositionDrawingSurfaceInterop = self
            .surface
            .cast()
            .call("QueryInterface(ICompositionDrawingSurfaceInterop)")?;
        unsafe {
            interop
                .Resize(SIZE {
                    cx: w as i32,
                    cy: h as i32,
                })
                .call("Resize")?;
        }
        self.sprite
            .SetSize(Vector2 {
                X: w as f32,
                Y: h as f32,
            })
            .call("SetSize")
    }

    /// Clear the surface and draw on it. One unit of `paint` is `scale` pixels.
    pub fn draw(
        &self,
        scale: f32,
        paint: impl FnOnce(&ID2D1DeviceContext) -> windows::core::Result<()>,
    ) -> Result<()> {
        let interop: ICompositionDrawingSurfaceInterop = self
            .surface
            .cast()
            .call("QueryInterface(ICompositionDrawingSurfaceInterop)")?;
        unsafe {
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = interop.BeginDraw(None, &mut offset).call("BeginDraw")?;
            dc.SetTransform(&Matrix3x2 {
                M11: scale,
                M12: 0.0,
                M21: 0.0,
                M22: scale,
                M31: offset.x as f32,
                M32: offset.y as f32,
            });
            dc.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            let painted = paint(&dc);
            interop.EndDraw().call("EndDraw")?;
            painted.call("drawing")
        }
    }
}

/// Add a backdrop at the top of `children`: the blurred desktop and the tint for glass, or
/// the tint without its alpha for a solid color.
pub fn add_backdrop(
    c: &Compositor,
    children: &VisualCollection,
    glass: bool,
    tint: Color,
) -> windows::core::Result<()> {
    let add = |brush: CompositionBrush| {
        let sprite = c.CreateSpriteVisual()?;
        sprite.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        sprite.SetBrush(&brush)?;
        children.InsertAtTop(&sprite)
    };
    if glass {
        add(c.CreateHostBackdropBrush()?.cast()?)?;
        add(c.CreateColorBrushWithColor(tint)?.cast()?)
    } else {
        add(c
            .CreateColorBrushWithColor(Color { A: 255, ..tint })?
            .cast()?)
    }
}

/// `rect` made smaller by `inset` on each side (larger if it is negative), with rounded
/// corners. The radius shrinks with a positive inset, so the curve stays parallel.
pub fn rounded(rect: D2D_RECT_F, inset: f32, radius: f32) -> D2D1_ROUNDED_RECT {
    let r = (radius - inset.max(0.0)).max(0.0);
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: rect.left + inset,
            top: rect.top + inset,
            right: rect.right - inset,
            bottom: rect.bottom - inset,
        },
        radiusX: r,
        radiusY: r,
    }
}

/// A rounded rectangle with a fill and a 1-unit border inside its edge, as buttons and
/// cards have.
pub fn panel(
    target: &ID2D1RenderTarget,
    rect: D2D_RECT_F,
    radius: f32,
    fill: &D2D1_COLOR_F,
    border: &D2D1_COLOR_F,
) -> windows::core::Result<()> {
    unsafe {
        target.FillRoundedRectangle(
            &rounded(rect, 0.0, radius),
            &target.CreateSolidColorBrush(fill, None)?,
        );
        target.DrawRoundedRectangle(
            &rounded(rect, 0.5, radius),
            &target.CreateSolidColorBrush(border, None)?,
            1.0,
            None,
        );
    }
    Ok(())
}

/// The size of `text` in `format`, when it wraps at `max_width`, rounded up.
pub fn text_size(
    dwrite: &IDWriteFactory,
    text: &str,
    format: &IDWriteTextFormat,
    max_width: f32,
) -> Result<(f32, f32)> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe {
        let layout = dwrite
            .CreateTextLayout(&wide, format, max_width.max(1.0), f32::MAX)
            .call("CreateTextLayout")?;
        layout.GetMetrics(&mut metrics).call("GetMetrics")?;
    }
    Ok((
        metrics.widthIncludingTrailingWhitespace.ceil(),
        metrics.height.ceil(),
    ))
}
