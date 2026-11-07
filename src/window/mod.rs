// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A general app window with a glass look, and the widgets in it.
//!
//! A [`Window`] has a background [`Material`], a tree of [widgets](crate::widget), and a
//! queue of [`WindowEvent`]s. It follows the light or dark mode, the accent color, the
//! display scale of its monitor, and the settings for transparency and high contrast, also
//! while it shows.
//!
//! ```no_run
//! use glasskit::Ui;
//! use glasskit::events::{Event, EventLoop};
//! use glasskit::widget::{Button, Column, Insets, Row, Spacer, Text, TextStyle};
//! use glasskit::window::{Window, WindowEvent};
//!
//! fn main() -> glasskit::Result<()> {
//!     let ui = Ui::new()?;
//!     let mut events: EventLoop<()> = EventLoop::new(&ui)?;
//!     let mut window = Window::new(&ui, "Hello", 420.0, 220.0)?;
//!     let title = window.add(Text::new("Hello, glass").style(TextStyle::Title));
//!     let spacer = window.add(Spacer::fill());
//!     let ok = window.add(Button::new("OK").accent());
//!     let buttons = window.add(Row::new().child(spacer).child(ok));
//!     let root = window.add(
//!         Column::new()
//!             .padding(Insets::all(24.0))
//!             .gap(16.0)
//!             .child(title)
//!             .child(buttons),
//!     );
//!     window.set_root(root);
//!     window.show();
//!     loop {
//!         if let Event::Window(id) = events.next(None)?
//!             && id == window.id()
//!         {
//!             while let Some(event) = window.next_event() {
//!                 match event {
//!                     WindowEvent::Clicked(button) if button == ok => return Ok(()),
//!                     WindowEvent::CloseRequested => return Ok(()),
//!                     _ => {}
//!                 }
//!             }
//!         }
//!     }
//! }
//! ```

mod draw;
mod scene;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE, DwmExtendFrameIntoClientArea,
    DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::ValidateRect;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
    VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, GetClientRect, IDC_ARROW, MINMAXINFO,
    PostMessageW, SIZE_MINIMIZED, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_CAPTURECHANGED, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSCOLORCHANGE, WM_THEMECHANGED, WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::Ui;
use crate::error::{Call, Result};
use crate::events::WindowId;
use crate::native::{self, NativeWindow, state_of};
use crate::render::Graphics;
use crate::theme::Theme;
use crate::ui::Notice;
use crate::widget::tree::{Content, Tree};
use crate::widget::{AnyWidget, Element, HasText, WidgetId};
use draw::{Formats, Interaction};
use scene::{Backdrop, Scene};

const CLASS_NAME: PCWSTR = w!("GlasskitWindow");
/// A private message: draw the window again, once for many changes.
const WM_REDRAW: u32 = WM_APP + 1;
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;
const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW;
const EX_STYLE: WINDOW_EX_STYLE = WINDOW_EX_STYLE(WS_EX_NOREDIRECTIONBITMAP.0 | WS_EX_APPWINDOW.0);

/// The background of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Material {
    /// Mica, the background of main windows in Windows 11. It takes a light tint of the
    /// desktop picture. On Windows 10, the window uses [`Material::Glass`].
    #[default]
    Mica,
    /// The blurred desktop behind the window, with a tint, as the popup.
    Glass,
    /// One opaque color.
    Solid,
}

/// Something that the user did in a window.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum WindowEvent {
    /// The user clicked a button, or pressed Space or Enter on it.
    Clicked(AnyWidget),
    /// The user clicked the close button or pressed Alt+F4. The window stays until the app
    /// drops it.
    CloseRequested,
}

/// A top-level window with a glass look and a tree of widgets.
///
/// Make it with a [`Ui`], add widgets, set the root, and [`show`](Window::show) it. The
/// event loop gives [`Event::Window`](crate::events::Event::Window) when it has events.
/// Dropping the window closes it.
pub struct Window {
    id: WindowId,
    native: NativeWindow<State>,
}

struct State {
    id: u32,
    scene: Scene,
    formats: Formats,
    tree: RefCell<Tree>,
    events: RefCell<VecDeque<WindowEvent>>,
    theme: Cell<Theme>,
    material: Cell<Material>,
    /// Pixels for each DIP.
    scale: Cell<f32>,
    /// The size of the client area in DIPs.
    size: Cell<(f32, f32)>,
    min_size: Cell<(f32, f32)>,
    now: Cell<Interaction>,
    tracking: Cell<bool>,
    redraw_posted: Cell<bool>,
    /// The GPU devices. They go last, after the scene.
    _gfx: Graphics,
}

impl Window {
    /// A hidden window with this title and a client area of this size, in DIPs. It shows
    /// after [`Window::show`].
    pub fn new(ui: &Ui, title: &str, width: f32, height: f32) -> Result<Self> {
        native::register_class(CLASS_NAME, Some(wndproc), ui.hinstance(), IDC_ARROW)?;
        let hwnd = unsafe {
            CreateWindowExW(
                EX_STYLE,
                CLASS_NAME,
                &HSTRING::from(title),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(ui.hinstance()),
                None,
            )
            .call("CreateWindowExW")?
        };
        // Until the state is attached, the window procedure uses the defaults. The window
        // drops if a step fails.
        let guard = DestroyOnError(Some(hwnd));
        let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
        let (outer_w, outer_h) = outer_size(width, height, scale);
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                0,
                0,
                outer_w,
                outer_h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .call("SetWindowPos")?;
        }
        let (px_w, px_h) = client_pixels(hwnd);
        let gfx = Graphics::new()?;
        let scene = Scene::new(&gfx, hwnd, px_w, px_h)?;
        let formats = Formats::new(&gfx)?;
        let id = ui.new_id();
        let state = State {
            id,
            scene,
            formats,
            tree: RefCell::new(Tree::default()),
            events: RefCell::new(VecDeque::new()),
            theme: Cell::new(Theme::current()),
            material: Cell::new(Material::default()),
            scale: Cell::new(scale),
            size: Cell::new((px_w as f32 / scale, px_h as f32 / scale)),
            min_size: Cell::new((0.0, 0.0)),
            now: Cell::new(Interaction::default()),
            tracking: Cell::new(false),
            redraw_posted: Cell::new(false),
            _gfx: gfx,
        };
        let native = NativeWindow::attach(hwnd, Rc::new(state));
        guard.disarm();
        let window = Self {
            id: WindowId(id),
            native,
        };
        window.state().apply_theme(hwnd);
        window.state().render();
        Ok(window)
    }

    fn state(&self) -> &State {
        self.native.state()
    }

    fn hwnd(&self) -> HWND {
        self.native.hwnd()
    }

    /// The id that [`Event::Window`](crate::events::Event::Window) gives for this window.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Show the window and bring it to the front.
    pub fn show(&self) {
        unsafe {
            // The first ShowWindow of a process can use the show state from the shortcut or
            // the parent process. The second call is exact.
            let _ = ShowWindow(self.hwnd(), SW_SHOWNORMAL);
            let _ = ShowWindow(self.hwnd(), SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd());
        }
    }

    /// Add a widget, and give its id. It shows when it is in the tree under the root.
    pub fn add<K>(&mut self, element: impl Into<Element<K>>) -> WidgetId<K> {
        let index = self.state().tree.borrow_mut().add(element.into().node);
        self.state().invalidate(self.hwnd());
        WidgetId::new(index)
    }

    /// The widget that fills the window. Its children, and their children, show.
    pub fn set_root(&mut self, root: impl Into<AnyWidget>) {
        let root = root.into().0;
        {
            let mut tree = self.state().tree.borrow_mut();
            if tree.get(root).is_some() {
                tree.root = Some(root);
            }
        }
        self.state().invalidate(self.hwnd());
    }

    /// The text of a text or the label of a button.
    pub fn text<K: HasText>(&self, id: WidgetId<K>) -> String {
        match self.state().tree.borrow().get(id.index).map(|n| &n.content) {
            Some(Content::Text { text, .. }) => text.clone(),
            Some(Content::Button { label, .. }) => label.clone(),
            _ => String::new(),
        }
    }

    /// Change the text of a text, or the label of a button.
    pub fn set_text<K: HasText>(&mut self, id: WidgetId<K>, value: &str) {
        if let Some(node) = self.state().tree.borrow_mut().get_mut(id.index) {
            match &mut node.content {
                Content::Text { text, .. } => *text = value.to_string(),
                Content::Button { label, .. } => *label = value.to_string(),
                _ => {}
            }
        }
        self.state().invalidate(self.hwnd());
    }

    /// Turn a widget on or off. A button that is off takes no clicks and no focus.
    pub fn set_enabled(&mut self, id: impl Into<AnyWidget>, enabled: bool) {
        let index = id.into().0;
        if let Some(node) = self.state().tree.borrow_mut().get_mut(index) {
            node.enabled = enabled;
        }
        let mut now = self.state().now.get();
        if !enabled && now.focus == Some(index) {
            now.focus = None;
            self.state().now.set(now);
        }
        self.state().invalidate(self.hwnd());
    }

    /// Give the keyboard focus to a widget that can take it.
    pub fn focus(&mut self, id: impl Into<AnyWidget>) {
        let index = id.into().0;
        if self.state().tree.borrow().focusable(index) {
            let mut now = self.state().now.get();
            now.focus = Some(index);
            self.state().now.set(now);
            self.state().invalidate(self.hwnd());
        }
    }

    /// Change the title.
    pub fn set_title(&self, title: &str) {
        unsafe {
            let _ = SetWindowTextW(self.hwnd(), &HSTRING::from(title));
        }
    }

    /// The smallest size of the client area that the user can make, in DIPs.
    pub fn set_min_size(&self, width: f32, height: f32) {
        self.state().min_size.set((width.max(0.0), height.max(0.0)));
    }

    /// The size of the client area, in DIPs.
    pub fn size(&self) -> (f32, f32) {
        self.state().size.get()
    }

    /// Change the background. If transparency effects are off, or high contrast is on, the
    /// window shows a solid color until they change.
    pub fn set_material(&self, material: Material) {
        self.state().material.set(material);
        self.state().apply_theme(self.hwnd());
        self.state().render();
    }

    /// The background that the app asked for.
    pub fn material(&self) -> Material {
        self.state().material.get()
    }

    /// The next thing that the user did, or `None` when there is nothing more now.
    pub fn next_event(&mut self) -> Option<WindowEvent> {
        self.state().events.borrow_mut().pop_front()
    }
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// Destroys a window when a step of its creation fails.
struct DestroyOnError(Option<HWND>);

impl DestroyOnError {
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for DestroyOnError {
    fn drop(&mut self) {
        if let Some(hwnd) = self.0 {
            native::destroy(hwnd);
        }
    }
}

impl State {
    fn push(&self, event: WindowEvent) {
        self.events.borrow_mut().push_back(event);
        crate::ui::notify(Notice::Window(self.id));
    }

    /// Draw again on the next turn of the loop. Many changes give one frame.
    fn invalidate(&self, hwnd: HWND) {
        if !self.redraw_posted.replace(true) {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_REDRAW, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// Lay out the widgets and draw them. A failed frame keeps the last one; the next
    /// change tries again.
    fn render(&self) {
        let (w, h) = self.size.get();
        self.tree.borrow_mut().layout(w, h, &self.formats);
        let tree = self.tree.borrow();
        let theme = self.theme.get();
        let _ = self.scene.draw(self.scale.get(), |dc| {
            draw::paint(dc, &tree, &theme, &self.formats, self.now.get())
        });
    }

    /// Read the theme again, and show the background that fits it.
    fn apply_theme(&self, hwnd: HWND) {
        let theme = Theme::current();
        self.theme.set(theme);
        set_dwm(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(theme.dark));
        let mut backdrop = match self.material.get() {
            _ if !theme.glass() => Backdrop::Solid,
            Material::Mica => Backdrop::Mica,
            Material::Glass => Backdrop::Glass,
            Material::Solid => Backdrop::Solid,
        };
        let mica = backdrop == Backdrop::Mica && set_backdrop_type(hwnd, DWMSBT_MAINWINDOW);
        if backdrop == Backdrop::Mica && !mica {
            backdrop = Backdrop::Glass;
        }
        if !mica {
            set_backdrop_type(hwnd, DWMSBT_NONE);
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
        let _ = self.scene.set_backdrop(backdrop, &theme);
    }

    fn set_interaction(&self, hwnd: HWND, change: impl FnOnce(&mut Interaction)) {
        let mut now = self.now.get();
        let before = (now.hover, now.pressed, now.focus, now.cues);
        change(&mut now);
        self.now.set(now);
        if before != (now.hover, now.pressed, now.focus, now.cues) {
            self.invalidate(hwnd);
        }
    }

    fn point(&self, lparam: LPARAM) -> (f32, f32) {
        let x = (lparam.0 & 0xFFFF) as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
        let s = self.scale.get();
        (x / s, y / s)
    }

    fn key(&self, hwnd: HWND, key: u16) -> bool {
        match key {
            k if k == VK_TAB.0 => {
                let back = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                let next = self.tree.borrow().next_focus(self.now.get().focus, back);
                self.set_interaction(hwnd, |now| {
                    now.focus = next;
                    now.cues = true;
                });
                true
            }
            k if k == VK_SPACE.0 || k == VK_RETURN.0 => {
                let focus = self.now.get().focus;
                if let Some(focus) = focus.filter(|&f| self.tree.borrow().focusable(f)) {
                    self.set_interaction(hwnd, |now| now.cues = true);
                    self.push(WindowEvent::Clicked(AnyWidget(focus)));
                }
                true
            }
            _ => false,
        }
    }
}

fn set_dwm(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: u32) -> bool {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const u32 as *const _,
            size_of::<u32>() as u32,
        )
        .is_ok()
    }
}

/// Ask DWM for a system backdrop. It fails before Windows 11 22H2.
fn set_backdrop_type(hwnd: HWND, kind: DWM_SYSTEMBACKDROP_TYPE) -> bool {
    set_dwm(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, kind.0 as u32)
}

/// The window size, in pixels, for a client area of this size in DIPs.
fn outer_size(width: f32, height: f32, scale: f32) -> (i32, i32) {
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

fn client_pixels(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    (
        (r.right - r.left).max(0) as u32,
        (r.bottom - r.top).max(0) as u32,
    )
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let Some(state) = state_of::<State>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_REDRAW => {
                state.redraw_posted.set(false);
                state.render();
                LRESULT(0)
            }
            WM_PAINT => {
                // The content is in the Composition tree, so there is nothing to paint here.
                let _ = ValidateRect(Some(hwnd), None);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                if wparam.0 != SIZE_MINIMIZED as usize {
                    let (w, h) = client_pixels(hwnd);
                    let s = state.scale.get();
                    state.size.set((w as f32 / s, h as f32 / s));
                    let _ = state.scene.resize(w, h);
                    // Draw now, so the window never shows a frame of the old size.
                    state.render();
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                state.scale.set((wparam.0 & 0xFFFF) as f32 / 96.0);
                let r = &*(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let (w, h) = client_pixels(hwnd);
                let s = state.scale.get();
                state.size.set((w as f32 / s, h as f32 / s));
                state.render();
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let (w, h) = state.min_size.get();
                if w > 0.0 || h > 0.0 {
                    let (ow, oh) = outer_size(w, h, state.scale.get());
                    let info = &mut *(lparam.0 as *mut MINMAXINFO);
                    info.ptMinTrackSize.x = ow;
                    info.ptMinTrackSize.y = oh;
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                state.push(WindowEvent::CloseRequested);
                LRESULT(0)
            }
            WM_SETTINGCHANGE
            | WM_THEMECHANGED
            | WM_SYSCOLORCHANGE
            | WM_DWMCOLORIZATIONCOLORCHANGED => {
                state.apply_theme(hwnd);
                state.render();
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_MOUSEMOVE => {
                if !state.tracking.get() {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    state.tracking.set(TrackMouseEvent(&mut tme).is_ok());
                }
                let (x, y) = state.point(lparam);
                let hit = state.tree.borrow().hit(x, y);
                state.set_interaction(hwnd, |now| now.hover = hit);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                state.tracking.set(false);
                state.set_interaction(hwnd, |now| now.hover = None);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = state.point(lparam);
                let hit = state.tree.borrow().hit(x, y);
                if hit.is_some() {
                    SetCapture(hwnd);
                }
                state.set_interaction(hwnd, |now| {
                    now.pressed = hit;
                    now.hover = hit;
                    if hit.is_some() {
                        now.focus = hit;
                        now.cues = false;
                    }
                });
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let pressed = state.now.get().pressed;
                let (x, y) = state.point(lparam);
                let hit = state.tree.borrow().hit(x, y);
                state.set_interaction(hwnd, |now| now.pressed = None);
                let _ = ReleaseCapture();
                if let Some(p) = pressed
                    && hit == Some(p)
                {
                    state.push(WindowEvent::Clicked(AnyWidget(p)));
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                state.set_interaction(hwnd, |now| now.pressed = None);
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if state.key(hwnd, wparam.0 as u16) {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
