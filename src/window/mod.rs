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
mod key;
mod pointer;
mod proc;
mod scene;
mod state;
mod text_box;
mod typing;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::DWMWA_USE_HOSTBACKDROPBRUSH;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, GetCursorPos, ICON_BIG, ICON_SMALL, IDC_ARROW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOZORDER, SendMessageW, SetWindowPos, SetWindowTextW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_SETICON, WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP, WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::Ui;
use crate::error::{Call, Result};
use crate::events::WindowId;
use crate::icon::{Icon, IconHandle};
use crate::menu::{self, MenuItem};
use crate::native::{self, NativeWindow};
use crate::render::Graphics;
use crate::theme::Theme;
use crate::widget::tree::{Content, Tree};
use crate::widget::{AnyId, Element, HasText, Id, SideMode, Tint, kind};
use draw::{Formats, Interaction};
use proc::wndproc;
use scene::Scene;
use state::{Clicks, Gpu, State, client_pixels, outer_size};

pub use key::{Key, KeyEvent};

const CLASS_NAME: PCWSTR = w!("GlasskitWindow");
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
    Clicked(AnyId),
    /// The user changed the text of a text box. Many changes in a row give one event.
    Changed(AnyId),
    /// The user pressed Enter in a text box with one line.
    Submitted(AnyId),
    /// The user pressed a key that no widget used, such as Esc, F5, or Ctrl+S.
    Key(KeyEvent),
    /// The user changed the mode of a [`SideBar`](crate::widget::SideBar) with its button.
    /// [`Window::side_mode`] gives the new mode.
    SideModeChanged(AnyId),
    /// The user dragged a [`Card`](crate::widget::Card) and dropped it on a container that
    /// is a drop target. The window does not move the card: the app does, with
    /// [`Window::set_children`].
    Dropped {
        /// The card.
        item: AnyId,
        /// The container.
        target: AnyId,
        /// The place of the drop among the children of the container, counted without the
        /// card: 0 is before the first child.
        index: usize,
    },
    /// The user asked for the menu of a widget that takes clicks: with the right button of
    /// the mouse, or with the menu key or Shift+F10 on the widget that has the focus. Show
    /// the menu with [`Window::menu`].
    ContextRequested(AnyId),
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
    /// The thread setup. It goes last, after the window.
    _ui: Ui,
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
        let guard = native::DestroyOnError::new(hwnd);
        // Without this, the brush of Material::Glass shows no desktop: the window is opaque.
        native::set_dwm_attribute(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH, 1);
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
            gpu: RefCell::new(Some(Gpu { scene, _gfx: gfx })),
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
            reveal: Cell::new(false),
            reveal_focus: Cell::new(false),
            icon: RefCell::new(None),
            clicks: Cell::new(Clicks::default()),
            grab: Cell::new(None),
            blink_since: Cell::new(Instant::now()),
            applied: Cell::new(None),
            shown: Cell::new(false),
            menu_at: Cell::new(None),
        };
        let native = NativeWindow::attach(hwnd, Rc::new(state));
        guard.disarm();
        let window = Self {
            id: WindowId(id),
            native,
            _ui: ui.clone(),
        };
        window.state().apply_theme(hwnd);
        window.state().render(hwnd);
        Ok(window)
    }

    fn state(&self) -> &State {
        self.native.state()
    }

    pub(crate) fn hwnd(&self) -> HWND {
        self.native.hwnd()
    }

    /// The id that [`Event::Window`](crate::events::Event::Window) gives for this window.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Show the window and bring it to the front. After [`Window::hide`], the window
    /// comes back in the place and the size that it had.
    pub fn show(&self) {
        if self.state().shown.replace(true) {
            native::show_again(self.hwnd());
        } else {
            native::show_window(self.hwnd());
        }
    }

    /// Hide the window, and its button on the taskbar. The window stays as it is, and
    /// [`Window::show`] shows it again. An app with a [`Tray`](crate::tray::Tray) can thus
    /// go on without a window.
    pub fn hide(&self) {
        native::hide_window(self.hwnd());
    }

    /// Show a menu and wait until the user picks an item or closes the menu. It gives the
    /// command of the item. After [`WindowEvent::ContextRequested`], the menu shows where
    /// the user asked for it; else it shows at the pointer.
    ///
    /// A [`MenuItem::Quit`] ends the app, as in the menu of a tray.
    pub fn menu<C: Clone>(&self, items: &[MenuItem<C>]) -> Option<C> {
        let at = self.state().menu_at.take().unwrap_or_else(|| {
            let mut at = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut at);
            }
            at
        });
        menu::popup_menu(self.hwnd(), at, items)
    }

    /// Add a widget, and give its id. It shows when it is in the tree under the root.
    pub fn add<K>(&mut self, element: impl Into<Element<K>>) -> Id<K> {
        let index = self.state().tree.borrow_mut().add(element.into().node);
        self.state().invalidate(self.hwnd());
        Id::new(index)
    }

    /// The widget that fills the window. Its children, and their children, show.
    pub fn set_root(&mut self, root: impl Into<AnyId>) {
        let root = root.into().0;
        {
            let mut tree = self.state().tree.borrow_mut();
            if tree.get(root).is_some() {
                tree.root = Some(root);
            }
        }
        self.state().invalidate(self.hwnd());
    }

    /// The text of a text or a text box, or the label of a button.
    pub fn text<K: HasText>(&self, id: Id<K>) -> String {
        match self.state().tree.borrow().get(id.index).map(|n| &n.content) {
            Some(Content::Text { text, .. }) => text.clone(),
            Some(Content::Button { label, .. } | Content::NavItem { label, .. }) => label.clone(),
            Some(Content::TextBox { edit, .. }) => edit.text().to_string(),
            _ => String::new(),
        }
    }

    /// Change the text of a text or a text box, or the label of a button. A change here
    /// gives no [`WindowEvent::Changed`].
    pub fn set_text<K: HasText>(&mut self, id: Id<K>, value: &str) {
        if let Some(node) = self.state().tree.borrow_mut().get_mut(id.index) {
            match &mut node.content {
                Content::Text { text, .. } => *text = value.to_string(),
                Content::Button { label, .. } | Content::NavItem { label, .. } => {
                    *label = value.to_string();
                }
                Content::TextBox { edit, scroll, .. } => {
                    edit.set_text(value);
                    *scroll = 0.0;
                }
                _ => {}
            }
        }
        self.state().invalidate(self.hwnd());
    }

    /// Turn a widget on or off. A button that is off takes no clicks and no focus.
    pub fn set_enabled(&mut self, id: impl Into<AnyId>, enabled: bool) {
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

    /// Show or hide a widget and all widgets in it. A hidden widget takes no space in its
    /// row, column, or stack, and no clicks and no focus. A new widget is visible.
    ///
    /// To change pages, put the pages in a [`Stack`](crate::widget::Stack), and show one
    /// of them.
    pub fn set_visible(&mut self, id: impl Into<AnyId>, visible: bool) {
        let index = id.into().0;
        if let Some(node) = self.state().tree.borrow_mut().get_mut(index) {
            node.visible = visible;
        }
        self.state().forget_hidden();
        self.state().invalidate(self.hwnd());
    }

    /// Give a container these children, in this order, in place of the ones that it has. A
    /// widget that was in another container moves out of it. A widget that is in no
    /// container does not show, and the app can put it in a container again later.
    ///
    /// The texts of a card with a title, and the button and the title of a side bar, stay.
    pub fn set_children<I, A>(&mut self, id: impl Into<AnyId>, children: I)
    where
        I: IntoIterator<Item = A>,
        A: Into<AnyId>,
    {
        let children: Vec<usize> = children.into_iter().map(|c| c.into().0).collect();
        self.state()
            .tree
            .borrow_mut()
            .set_children(id.into().0, &children);
        self.state().forget_hidden();
        self.state().invalidate(self.hwnd());
    }

    /// The title and the body of a card. They are empty for a card without a text.
    pub fn card_text(&self, id: Id<kind::Card>) -> (String, String) {
        let text = self.state().tree.borrow().card_text(id.index);
        text.unwrap_or_default()
    }

    /// Change the title and the body of a card that was made with a title or a body. A card
    /// with an empty body shows only its title.
    pub fn set_card_text(&mut self, id: Id<kind::Card>, title: &str, body: &str) {
        self.state()
            .tree
            .borrow_mut()
            .set_card_text(id.index, title, body);
        self.state().invalidate(self.hwnd());
    }

    /// Give a card a color, or take its color away.
    pub fn set_tint(&mut self, id: Id<kind::Card>, tint: Option<Tint>) {
        self.state().tree.borrow_mut().set_tint(id.index, tint);
        self.state().invalidate(self.hwnd());
    }

    /// Show or hide the pin in the corner of a card.
    pub fn set_pinned(&mut self, id: Id<kind::Card>, pinned: bool) {
        self.state().tree.borrow_mut().set_pinned(id.index, pinned);
        self.state().invalidate(self.hwnd());
    }

    /// Change how much of a side bar shows. It gives no
    /// [`WindowEvent::SideModeChanged`].
    pub fn set_side_mode(&mut self, id: Id<kind::SideBar>, mode: SideMode) {
        self.state().tree.borrow_mut().set_side_mode(id.index, mode);
        self.state().forget_hidden();
        self.state().invalidate(self.hwnd());
    }

    /// How much of a side bar shows.
    pub fn side_mode(&self, id: Id<kind::SideBar>) -> SideMode {
        let mode = self.state().tree.borrow().side_mode(id.index);
        mode.unwrap_or_default()
    }

    /// Whether [`set_visible`](Window::set_visible) shows the widget. A visible widget in
    /// a hidden container does not show.
    pub fn is_visible(&self, id: impl Into<AnyId>) -> bool {
        let tree = self.state().tree.borrow();
        tree.get(id.into().0).is_some_and(|n| n.visible)
    }

    /// Mark a navigation item as the one whose page shows, or remove the mark.
    pub fn set_selected(&mut self, id: Id<kind::NavItem>, selected: bool) {
        if let Some(Content::NavItem { selected: s, .. }) = self
            .state()
            .tree
            .borrow_mut()
            .get_mut(id.index)
            .map(|n| &mut n.content)
        {
            *s = selected;
        }
        self.state().invalidate(self.hwnd());
    }

    /// Whether a navigation item has the mark.
    pub fn is_selected(&self, id: Id<kind::NavItem>) -> bool {
        matches!(
            self.state().tree.borrow().get(id.index).map(|n| &n.content),
            Some(Content::NavItem { selected: true, .. })
        )
    }

    /// Give the keyboard focus to a widget that can take it.
    pub fn focus(&mut self, id: impl Into<AnyId>) {
        let index = id.into().0;
        if self.state().tree.borrow().focusable(index) {
            let mut now = self.state().now.get();
            now.focus = Some(index);
            self.state().now.set(now);
            self.state().invalidate(self.hwnd());
        }
    }

    /// Show this picture in the title bar, the taskbar, and Alt+Tab. Windows scales it to
    /// each place, so give a large one, such as 64 by 64 pixels.
    pub fn set_icon(&self, icon: &Icon) -> Result<()> {
        let handle = IconHandle::new(icon)?;
        unsafe {
            for kind in [ICON_SMALL, ICON_BIG] {
                SendMessageW(
                    self.hwnd(),
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(handle.0.0 as isize)),
                );
            }
        }
        // The old icon goes only now, after the window uses the new one.
        self.state().icon.replace(Some(handle));
        Ok(())
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
        if self.state().apply_theme(self.hwnd()) {
            self.state().render(self.hwnd());
        }
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
