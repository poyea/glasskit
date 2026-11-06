// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

use std::time::Duration;

use crate::error::{Call, Result};
use windows::Foundation::{Size, TimeSpan};
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionDrawingSurface, CompositionGraphicsDevice, Compositor, ContainerVisual,
};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Device, ID2D1DeviceContext,
    ID2D1Factory1,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorDesktopInterop, ICompositorInterop,
};
use windows::core::{HSTRING, Interface, w};
use windows_numerics::{Matrix3x2, Vector2};

use crate::theme::{Theme, rgba};

/// The GPU devices. Keep them only while a popup shows, because they use much memory.
pub struct Graphics {
    compositor: Compositor,
    graphics_device: CompositionGraphicsDevice,
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
        unsafe {
            let format = self
                .dwrite
                .CreateTextFormat(
                    w!("Segoe UI Variable Text"),
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

pub struct Scene {
    compositor: Compositor,
    root: ContainerVisual,
    surface: CompositionDrawingSurface,
    _target: DesktopWindowTarget,
}

impl Scene {
    /// The scene clips its own rounded corners. Do not let DWM round the window, because DWM
    /// shows its border and shadow immediately, and they do not fade in with the scene.
    pub fn new(
        gfx: &Graphics,
        hwnd: HWND,
        width: f32,
        height: f32,
        corner_radius: f32,
        theme: &Theme,
    ) -> Result<Self> {
        Self::build(gfx, hwnd, width, height, corner_radius, theme).call("creating the popup scene")
    }

    fn build(
        gfx: &Graphics,
        hwnd: HWND,
        width: f32,
        height: f32,
        corner_radius: f32,
        theme: &Theme,
    ) -> windows::core::Result<Self> {
        let c = &gfx.compositor;
        let target = unsafe {
            c.cast::<ICompositorDesktopInterop>()?
                .CreateDesktopWindowTarget(hwnd, true)?
        };

        let full = Vector2 { X: 1.0, Y: 1.0 };
        let root = c.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(full)?;
        root.SetOpacity(0.0)?;
        let shape = c.CreateRoundedRectangleGeometry()?;
        shape.SetSize(Vector2 {
            X: width,
            Y: height,
        })?;
        shape.SetCornerRadius(Vector2 {
            X: corner_radius,
            Y: corner_radius,
        })?;
        root.SetClip(&c.CreateGeometricClipWithGeometry(&shape)?)?;

        let backdrop = c.CreateSpriteVisual()?;
        backdrop.SetRelativeSizeAdjustment(full)?;
        backdrop.SetBrush(&c.CreateHostBackdropBrush()?)?;

        let tint = c.CreateSpriteVisual()?;
        tint.SetRelativeSizeAdjustment(full)?;
        tint.SetBrush(&c.CreateColorBrushWithColor(theme.tint)?)?;

        let surface = gfx.graphics_device.CreateDrawingSurface(
            Size {
                Width: width,
                Height: height,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )?;
        let content = c.CreateSpriteVisual()?;
        content.SetSize(Vector2 {
            X: width,
            Y: height,
        })?;
        content.SetBrush(&c.CreateSurfaceBrushWithSurface(&surface)?)?;

        let children = root.Children()?;
        children.InsertAtTop(&backdrop)?;
        children.InsertAtTop(&tint)?;
        children.InsertAtTop(&content)?;
        target.SetRoot(&root)?;

        Ok(Self {
            compositor: c.clone(),
            root,
            surface,
            _target: target,
        })
    }

    pub fn fade(&self, from: f32, to: f32, duration: Duration) -> Result<()> {
        self.animate_opacity(from, to, duration)
            .call("starting the popup animation")
    }

    fn animate_opacity(&self, from: f32, to: f32, duration: Duration) -> windows::core::Result<()> {
        let c = &self.compositor;
        let anim = c.CreateScalarKeyFrameAnimation()?;
        anim.InsertKeyFrame(0.0, from)?;
        anim.InsertKeyFrameWithEasingFunction(1.0, to, &c.CreateLinearEasingFunction()?)?;
        anim.SetDuration(TimeSpan::from(duration))?;
        self.root.StartAnimation(&HSTRING::from("Opacity"), &anim)?;
        Ok(())
    }

    pub fn draw(
        &self,
        paint: impl FnOnce(&ID2D1DeviceContext) -> windows::core::Result<()>,
    ) -> Result<()> {
        let interop: ICompositionDrawingSurfaceInterop = self
            .surface
            .cast()
            .call("QueryInterface(ICompositionDrawingSurfaceInterop)")?;
        unsafe {
            let mut offset = POINT::default();
            let dc: ID2D1DeviceContext = interop.BeginDraw(None, &mut offset).call("BeginDraw")?;
            dc.SetTransform(&Matrix3x2::translation(offset.x as f32, offset.y as f32));
            dc.Clear(Some(&rgba(0.0, 0.0, 0.0, 0.0)));
            let painted = paint(&dc);
            interop.EndDraw().call("EndDraw")?;
            painted.call("drawing the popup")
        }
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
