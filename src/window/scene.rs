// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The Composition tree of a window: a background for the material, and the drawn content
//! on top of it.

use windows::Foundation::Size;
use windows::Graphics::DirectX::{DirectXAlphaMode, DirectXPixelFormat};
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{
    CompositionDrawingSurface, Compositor, ContainerVisual, SpriteVisual,
};
use windows::Win32::Foundation::{HWND, POINT, SIZE};
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::System::WinRT::Composition::{
    ICompositionDrawingSurfaceInterop, ICompositorDesktopInterop,
};
use windows::core::Interface;
use windows_numerics::{Matrix3x2, Vector2};

use crate::error::{Call, Result};
use crate::render::Graphics;
use crate::theme::{Theme, rgba};

/// The background that a window uses now. It can differ from the material that the app
/// asks for, for example when transparency effects are off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Backdrop {
    /// Windows draws Mica behind the content.
    Mica,
    /// The blurred desktop with a tint, as the popup.
    Glass,
    /// One opaque color.
    Solid,
}

pub(super) struct Scene {
    compositor: Compositor,
    background: ContainerVisual,
    content: SpriteVisual,
    surface: CompositionDrawingSurface,
    _target: DesktopWindowTarget,
}

impl Scene {
    pub(super) fn new(gfx: &Graphics, hwnd: HWND, width: u32, height: u32) -> Result<Self> {
        Self::build(gfx, hwnd, width, height).call("creating the window scene")
    }

    fn build(gfx: &Graphics, hwnd: HWND, width: u32, height: u32) -> windows::core::Result<Self> {
        let c = &gfx.compositor;
        let target = unsafe {
            c.cast::<ICompositorDesktopInterop>()?
                .CreateDesktopWindowTarget(hwnd, false)?
        };
        let full = Vector2 { X: 1.0, Y: 1.0 };
        let root = c.CreateContainerVisual()?;
        root.SetRelativeSizeAdjustment(full)?;
        let background = c.CreateContainerVisual()?;
        background.SetRelativeSizeAdjustment(full)?;

        let surface = gfx.graphics_device.CreateDrawingSurface(
            Size {
                Width: width.max(1) as f32,
                Height: height.max(1) as f32,
            },
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            DirectXAlphaMode::Premultiplied,
        )?;
        let content = c.CreateSpriteVisual()?;
        content.SetSize(Vector2 {
            X: width.max(1) as f32,
            Y: height.max(1) as f32,
        })?;
        content.SetBrush(&c.CreateSurfaceBrushWithSurface(&surface)?)?;

        let children = root.Children()?;
        children.InsertAtTop(&background)?;
        children.InsertAtTop(&content)?;
        target.SetRoot(&root)?;
        Ok(Self {
            compositor: c.clone(),
            background,
            content,
            surface,
            _target: target,
        })
    }

    /// Show this background.
    pub(super) fn set_backdrop(&self, backdrop: Backdrop, theme: &Theme) -> Result<()> {
        self.build_backdrop(backdrop, theme)
            .call("changing the window background")
    }

    fn build_backdrop(&self, backdrop: Backdrop, theme: &Theme) -> windows::core::Result<()> {
        let c = &self.compositor;
        let full = Vector2 { X: 1.0, Y: 1.0 };
        let children = self.background.Children()?;
        children.RemoveAll()?;
        let sprite = |brush: &windows::UI::Composition::CompositionBrush| {
            let s = c.CreateSpriteVisual()?;
            s.SetRelativeSizeAdjustment(full)?;
            s.SetBrush(brush)?;
            children.InsertAtTop(&s)
        };
        match backdrop {
            Backdrop::Mica => {}
            Backdrop::Glass => {
                sprite(&c.CreateHostBackdropBrush()?.cast()?)?;
                sprite(&c.CreateColorBrushWithColor(theme.tint)?.cast()?)?;
            }
            Backdrop::Solid => {
                let mut color = theme.tint;
                color.A = 255;
                sprite(&c.CreateColorBrushWithColor(color)?.cast()?)?;
            }
        }
        Ok(())
    }

    /// Make the drawing surface this many pixels.
    pub(super) fn resize(&self, width: u32, height: u32) -> Result<()> {
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
        self.content
            .SetSize(Vector2 {
                X: w as f32,
                Y: h as f32,
            })
            .call("SetSize")
    }

    /// Draw the content. `paint` draws in DIPs: the scene scales them to pixels.
    pub(super) fn draw(
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
            painted.call("drawing the window")
        }
    }
}
