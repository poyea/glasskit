// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The state of a window, and how it draws.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DwmExtendFrameIntoClientArea,
};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PostMessageW};

use super::draw::{self, Formats, Interaction};
use super::proc::WM_REDRAW;
use super::scene::{Backdrop, Scene};
use super::{EX_STYLE, Material, STYLE, WindowEvent};
use crate::error::Result;
use crate::icon::IconHandle;
use crate::native;
use crate::render::Graphics;
use crate::theme::Theme;
use crate::ui::Notice;
use crate::widget::AnyId;
use crate::widget::tree::{Content, Tree};

pub(super) struct State {
    pub(super) id: u32,
    /// The GPU part. It is made again after a device loss.
    pub(super) gpu: RefCell<Option<Gpu>>,
    pub(super) formats: Formats,
    pub(super) tree: RefCell<Tree>,
    pub(super) events: RefCell<VecDeque<WindowEvent>>,
    pub(super) theme: Cell<Theme>,
    pub(super) material: Cell<Material>,
    /// Pixels for each DIP.
    pub(super) scale: Cell<f32>,
    /// The size of the client area in DIPs.
    pub(super) size: Cell<(f32, f32)>,
    pub(super) min_size: Cell<(f32, f32)>,
    pub(super) now: Cell<Interaction>,
    pub(super) tracking: Cell<bool>,
    pub(super) redraw_posted: Cell<bool>,
    /// Scroll the focused text box to its caret at the next layout.
    pub(super) reveal: Cell<bool>,
    /// Scroll the focused widget into view at the next layout.
    pub(super) reveal_focus: Cell<bool>,
    /// The icon of the title bar and the taskbar.
    pub(super) icon: RefCell<Option<IconHandle>>,
    /// The last press of the left button, to count double and triple clicks.
    pub(super) clicks: Cell<Clicks>,
    /// The widget that the pointer holds, to drag it.
    pub(super) grab: Cell<Option<Grab>>,
    /// When the caret started to blink.
    pub(super) blink_since: Cell<Instant>,
    /// The theme and material that the background shows now.
    pub(super) applied: Cell<Option<(Theme, Material)>>,
    /// [`Window::show`] ran.
    pub(super) shown: Cell<bool>,
    /// Where the user asked for a menu, on the screen, in pixels.
    pub(super) menu_at: Cell<Option<POINT>>,
}

/// A press of the left button on a widget that the user can drag.
#[derive(Debug, Clone, Copy)]
pub(super) struct Grab {
    pub(super) item: usize,
    /// Where the press was, in DIPs.
    pub(super) start: (f32, f32),
    /// The pointer moved far enough, so the widget follows it.
    pub(super) dragging: bool,
}

/// The last press of the left button.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Clicks {
    pub(super) count: u32,
    /// The message time, in milliseconds.
    pub(super) time: u32,
    /// In pixels.
    pub(super) at: (i32, i32),
}

/// The scene and the GPU devices that it draws with.
pub(super) struct Gpu {
    pub(super) scene: Scene,
    /// The devices go last, after the scene.
    pub(super) _gfx: Graphics,
}

impl State {
    pub(super) fn push(&self, event: WindowEvent) {
        let mut events = self.events.borrow_mut();
        if matches!(event, WindowEvent::Changed(_)) && events.back() == Some(&event) {
            return;
        }
        events.push_back(event);
        drop(events);
        crate::ui::notify(Notice::Window(self.id));
    }

    /// A click on a widget, or Space or Enter on it.
    pub(super) fn click(&self, hwnd: HWND, index: usize) {
        let toggle = matches!(
            self.tree.borrow().get(index).map(|n| &n.content),
            Some(Content::SideToggle)
        );
        if !toggle {
            self.push(WindowEvent::Clicked(AnyId(index)));
            return;
        }
        let bar = self.tree.borrow_mut().step_side(index);
        if let Some(bar) = bar {
            self.forget_hidden();
            self.invalidate(hwnd);
            self.push(WindowEvent::SideModeChanged(AnyId(bar)));
        }
    }

    /// Take the focus, the hover, and the press away from widgets that do not show.
    pub(super) fn forget_hidden(&self) {
        let mut now = self.now.get();
        {
            let tree = self.tree.borrow();
            let shown = |i: Option<usize>| i.filter(|&i| tree.shown(i));
            now.focus = shown(now.focus);
            now.hover = shown(now.hover);
            now.pressed = shown(now.pressed);
        }
        self.now.set(now);
    }

    /// Draw again on the next turn of the loop. Many changes give one frame.
    pub(super) fn invalidate(&self, hwnd: HWND) {
        if !self.redraw_posted.replace(true) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_REDRAW, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Lay out the widgets and draw them. A failed frame keeps the last one; the next
    /// change tries again.
    pub(super) fn render(&self, hwnd: HWND) {
        let (w, h) = self.size.get();
        self.tree.borrow_mut().layout(w, h, &self.formats);
        if self.reveal_focus.take()
            && let Some(focus) = self.now.get().focus
            && self.tree.borrow_mut().reveal(focus)
        {
            self.tree.borrow_mut().layout(w, h, &self.formats);
        }
        if self.reveal.take() {
            self.reveal_caret();
        }
        if self.draw().is_err_and(|err| err.device_lost()) && self.renew_gpu(hwnd).is_ok() {
            let _ = self.draw();
        }
    }

    fn draw(&self) -> Result<()> {
        let tree = self.tree.borrow();
        let theme = self.theme.get();
        let gpu = self.gpu.borrow();
        let Some(gpu) = gpu.as_ref() else {
            return Ok(());
        };
        gpu.scene.draw(self.scale.get(), |dc| {
            draw::paint(dc, &tree, &theme, &self.formats, self.now.get())
        })
    }

    /// Make the GPU part again, after a device loss. The old one goes first, because a
    /// window can have only one Composition target.
    fn renew_gpu(&self, hwnd: HWND) -> Result<()> {
        self.gpu.borrow_mut().take();
        let gfx = Graphics::new()?;
        let (w, h) = client_pixels(hwnd);
        let scene = Scene::new(&gfx, hwnd, w, h)?;
        *self.gpu.borrow_mut() = Some(Gpu { scene, _gfx: gfx });
        self.applied.set(None);
        self.apply_theme(hwnd);
        Ok(())
    }

    /// Change the scene, if there is one. A failure keeps the scene as it was.
    pub(super) fn with_scene(&self, change: impl FnOnce(&Scene) -> Result<()>) {
        if let Some(gpu) = self.gpu.borrow().as_ref() {
            let _ = change(&gpu.scene);
        }
    }

    /// Read the theme again, and show the background that fits it.
    /// It gives false, and does nothing, when the theme and the material are as before.
    pub(super) fn apply_theme(&self, hwnd: HWND) -> bool {
        let theme = Theme::current();
        let wanted = Some((theme, self.material.get()));
        if self.applied.get() == wanted {
            return false;
        }
        self.applied.set(wanted);
        self.theme.set(theme);
        native::set_dwm_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(theme.dark));
        let mut backdrop = match self.material.get() {
            _ if !theme.glass() => Backdrop::Solid,
            Material::Mica => Backdrop::Mica,
            Material::Glass => Backdrop::Glass,
            Material::Solid => Backdrop::Solid,
        };
        // Mica stays below all materials, and Glass and Solid cover it. A removal of Mica
        // shows the desktop through the window until the new background is there.
        let mica = set_backdrop_type(hwnd, DWMSBT_MAINWINDOW);
        if backdrop == Backdrop::Mica && !mica {
            backdrop = Backdrop::Glass;
        }
        // Mica shows only where the frame reaches into the client area.
        let reach = if mica { -1 } else { 0 };
        let margins = MARGINS {
            cxLeftWidth: reach,
            cxRightWidth: reach,
            cyTopHeight: reach,
            cyBottomHeight: reach,
        };
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        }
        self.with_scene(|scene| scene.set_backdrop(backdrop, &theme));
        true
    }

    pub(super) fn set_interaction(&self, hwnd: HWND, change: impl FnOnce(&mut Interaction)) {
        let before = self.now.get();
        let mut now = before;
        change(&mut now);
        self.now.set(now);
        if before != now {
            self.invalidate(hwnd);
        }
        if (before.focus, before.active) != (now.focus, now.active) {
            self.restart_blink(hwnd);
        }
    }

    pub(super) fn point(&self, lparam: LPARAM) -> (f32, f32) {
        let (x, y) = native::point(lparam);
        let (x, y) = (x as f32, y as f32);
        let s = self.scale.get();
        (x / s, y / s)
    }

    /// A point on the screen, in DIPs in the client area.
    pub(super) fn screen_point(&self, hwnd: HWND, mut p: POINT) -> (f32, f32) {
        unsafe {
            let _ = ScreenToClient(hwnd, &mut p);
        }
        let s = self.scale.get();
        (p.x as f32 / s, p.y as f32 / s)
    }
}

/// Ask DWM for a system backdrop. It fails before Windows 11 22H2.
fn set_backdrop_type(hwnd: HWND, kind: DWM_SYSTEMBACKDROP_TYPE) -> bool {
    native::set_dwm_attribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, kind.0 as u32)
}

/// The window size, in pixels, for a client area of this size in DIPs.
pub(super) fn outer_size(width: f32, height: f32, scale: f32) -> (i32, i32) {
    let dpi = (scale * 96.0).round() as u32;
    let mut r = RECT {
        left: 0,
        top: 0,
        right: (width * scale).round() as i32,
        bottom: (height * scale).round() as i32,
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut r, STYLE, false, EX_STYLE, dpi);
    }
    (r.right - r.left, r.bottom - r.top)
}

pub(super) fn client_pixels(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    (
        (r.right - r.left).max(0) as u32,
        (r.bottom - r.top).max(0) as u32,
    )
}
