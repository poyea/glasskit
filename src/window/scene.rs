// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The Composition tree of a window: a background for the material, and the drawn content
//! on top of it.

use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::core::Interface;
use windows_numerics::Vector2;

use crate::error::{Call, Result};
use crate::render::{self, Graphics, Surface};
use crate::theme::Theme;

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
    surface: Surface,
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
        let surface = Surface::new(gfx, width as f32, height as f32)?;
        let children = root.Children()?;
        children.InsertAtTop(&background)?;
        children.InsertAtTop(surface.sprite())?;
        target.SetRoot(&root)?;
        Ok(Self {
            compositor: c.clone(),
            background,
            surface,
            _target: target,
        })
    }

    /// Show this background.
    pub(super) fn set_backdrop(&self, backdrop: Backdrop, theme: &Theme) -> Result<()> {
        let children = self.background.Children().call("Children")?;
        children.RemoveAll().call("RemoveAll")?;
        match backdrop {
            Backdrop::Mica => Ok(()),
            Backdrop::Glass | Backdrop::Solid => render::add_backdrop(
                &self.compositor,
                &children,
                backdrop == Backdrop::Glass,
                theme.tint,
            )
            .call("changing the window background"),
        }
    }

    /// Make the drawing surface this many pixels.
    pub(super) fn resize(&self, width: u32, height: u32) -> Result<()> {
        self.surface.resize(width, height)
    }

    /// Draw the content. `paint` draws in DIPs: the scene scales them to pixels.
    pub(super) fn draw(
        &self,
        scale: f32,
        paint: impl FnOnce(&ID2D1DeviceContext) -> windows::core::Result<()>,
    ) -> Result<()> {
        self.surface.draw(scale, paint)
    }
}
