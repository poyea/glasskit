// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The Composition scene of a popup: the glass backdrop, the content, the rounded clip, and
//! the fade.

use std::time::Duration;

use windows::Foundation::TimeSpan;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::ID2D1DeviceContext;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::core::{HSTRING, Interface};
use windows_numerics::Vector2;

use crate::error::{Call, Result};
use crate::render::{self, Graphics, Surface};
use crate::theme::Theme;

pub struct Scene {
    compositor: Compositor,
    root: ContainerVisual,
    surface: Surface,
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

        let surface = Surface::new(gfx, width, height)?;
        let children = root.Children()?;
        render::add_backdrop(c, &children, true, theme.tint)?;
        children.InsertAtTop(surface.sprite())?;
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
        // The popup lays out in pixels.
        self.surface.draw(1.0, paint)
    }
}
