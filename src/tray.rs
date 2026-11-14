// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A notification-area (tray) icon with a menu.
//!
//! A tray belongs to an [`EventLoop`]: when the user picks a menu command,
//! [`EventLoop::next`](crate::events::EventLoop::next) gives
//! [`Event::Command`](crate::events::Event::Command).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIN_BALLOONUSERCLICK, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetCursorPos, IDC_ARROW, PostQuitMessage, RegisterWindowMessageW,
    TPM_BOTTOMALIGN, TPM_RIGHTALIGN, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP,
};
use windows::core::{PCWSTR, w};

use crate::error::{Error, Result};
use crate::events::{EventLoop, Source};
use crate::icon::IconHandle;
use crate::menu::{Choice, Entry, allow_dark_menus, to_entries, track};
use crate::native::{self, NativeWindow};

pub use crate::icon::Icon;
pub use crate::menu::MenuItem;

const CLASS_NAME: PCWSTR = w!("GlasskitTray");
const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;

/// The state of the window procedure. It has no type parameter, so one window class serves
/// all command types.
struct Shared {
    entries: RefCell<Vec<Entry>>,
    /// It changes with each new menu. A choice from an older menu is not used.
    generation: Cell<u64>,
    chosen: RefCell<VecDeque<(u64, usize)>>,
    /// A click on the icon or on a notification gives a command, and opens no menu.
    takes_clicks: Cell<bool>,
    clicked: Cell<bool>,
    icon_data: RefCell<NOTIFYICONDATAW>,
    taskbar_created: u32,
}

/// What the event loop asks for commands: the window state and the commands of the menu.
struct Core<C> {
    shared: Rc<Shared>,
    commands: RefCell<Vec<C>>,
    /// The command of a click on the icon or on a notification.
    click: RefCell<Option<C>>,
}

impl<C: Clone> Source<C> for Core<C> {
    fn take_command(&self) -> Option<C> {
        if self.shared.clicked.take()
            && let Some(command) = self.click.borrow().clone()
        {
            return Some(command);
        }
        loop {
            let (generation, index) = self.shared.chosen.borrow_mut().pop_front()?;
            if generation == self.shared.generation.get()
                && let Some(command) = self.commands.borrow().get(index)
            {
                return Some(command.clone());
            }
        }
    }
}

/// The icon, its window, and the state of the window procedure. None of it depends on the
/// command type, so the compiler builds this code once.
struct Native {
    window: NativeWindow<Shared>,
    _icon: IconHandle,
}

impl Native {
    fn new(hinstance: HINSTANCE, tooltip: &str, icon: &Icon) -> Result<Self> {
        native::register_class(CLASS_NAME, Some(wndproc), hinstance, IDC_ARROW)?;
        let icon = IconHandle::new(icon)?;
        // Use a hidden top-level window. A message-only window does not get the
        // TaskbarCreated message when Explorer restarts.
        let hwnd = native::create_hidden(CLASS_NAME, hinstance)?;
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon.0,
            ..Default::default()
        };
        set_tip(&mut data, tooltip);
        let shared = Rc::new(Shared {
            entries: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            chosen: RefCell::new(VecDeque::new()),
            takes_clicks: Cell::new(false),
            clicked: Cell::new(false),
            icon_data: RefCell::new(data),
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        });
        let native = Self {
            window: NativeWindow::attach(hwnd, shared),
            _icon: icon,
        };
        allow_dark_menus(hwnd);
        let added =
            unsafe { Shell_NotifyIconW(NIM_ADD, &*native.window.state().icon_data.borrow()) };
        if !added.as_bool() {
            return Err(Error::TrayIconNotAdded);
        }
        Ok(native)
    }

    fn set_entries(&self, entries: Vec<Entry>) {
        let shared = self.window.state();
        shared.entries.replace(entries);
        shared.generation.set(shared.generation.get() + 1);
    }

    fn set_tooltip(&self, tooltip: &str) {
        let mut data = self.window.state().icon_data.borrow_mut();
        set_tip(&mut data, tooltip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &*data);
        }
    }

    fn notify(&self, title: &str, text: &str) {
        // A copy, so that a new icon after a restart of Explorer shows no old notification.
        let mut data = *self.window.state().icon_data.borrow();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_USER;
        data.hBalloonIcon = data.hIcon;
        copy_text(&mut data.szInfoTitle, title);
        copy_text(&mut data.szInfo, text);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &*self.window.state().icon_data.borrow());
        }
    }
}

/// A tray icon and its menu. `C` is the command type of the app, for example an enum.
///
/// It gives its commands to the [`EventLoop`] that it was made with.
pub struct Tray<C> {
    native: Native,
    core: Rc<Core<C>>,
}

impl<C: Clone + 'static> Tray<C> {
    /// Add an icon with this tooltip to the notification area. The menu is empty until
    /// [`Tray::set_menu`].
    pub fn new(events: &EventLoop<C>, tooltip: &str, icon: &Icon) -> Result<Self> {
        let native = Native::new(events.ui().hinstance(), tooltip, icon)?;
        let core = Rc::new(Core {
            shared: native.window.share(),
            commands: RefCell::new(Vec::new()),
            click: RefCell::new(None),
        });
        let source: Weak<dyn Source<C>> = Rc::downgrade(&core) as Weak<dyn Source<C>>;
        events.add_source(source);
        Ok(Self { native, core })
    }

    /// Replace the menu. A choice that the user made in the old menu and that the event loop
    /// did not give yet is dropped.
    pub fn set_menu(&mut self, items: &[MenuItem<C>]) {
        let mut commands = Vec::new();
        let entries = to_entries(items, &mut commands);
        self.core.commands.replace(commands);
        self.native.set_entries(entries);
    }

    /// Change the text that shows when the pointer is on the icon. Windows shows up to 127
    /// characters.
    pub fn set_tooltip(&self, tooltip: &str) {
        self.native.set_tooltip(tooltip);
    }

    /// Give a command for a click with the left button on the icon, and for a click on a
    /// [notification](Tray::notify), for example to show the window of the app. The right
    /// button still opens the menu. With `None`, the left button opens the menu, as at the
    /// start.
    pub fn set_click(&mut self, command: Option<C>) {
        self.core.shared.takes_clicks.set(command.is_some());
        self.core.click.replace(command);
    }

    /// Show a notification of Windows with the icon of the tray. Windows shows up to 63
    /// characters of the title and 255 characters of the text, and shows nothing while the
    /// user has notifications off, for example with Do not disturb.
    pub fn notify(&self, title: &str, text: &str) {
        self.native.notify(title, text);
    }
}

fn set_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    copy_text(&mut data.szTip, tip);
}

/// Put a text in a field of fixed length, cut to the length, with a zero at its end.
fn copy_text(field: &mut [u16], text: &str) {
    let wide: Vec<u16> = text.encode_utf16().take(field.len() - 1).collect();
    field.fill(0);
    field[..wide.len()].copy_from_slice(&wide);
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let Some(shared) = native::state_of::<Shared>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                let takes_clicks = shared.takes_clicks.get();
                if takes_clicks && (event == WM_LBUTTONUP || event == NIN_BALLOONUSERCLICK) {
                    shared.clicked.set(true);
                } else if event == WM_LBUTTONUP || event == WM_RBUTTONUP {
                    show_menu(hwnd, shared);
                }
                LRESULT(0)
            }
            m if m != 0 && m == shared.taskbar_created => {
                let data = *shared.icon_data.borrow();
                let _ = Shell_NotifyIconW(NIM_ADD, &data);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn show_menu(hwnd: HWND, shared: &Shared) {
    let entries = shared.entries.borrow().clone();
    let generation = shared.generation.get();
    let mut at = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut at);
    }
    match track(hwnd, &entries, at, TPM_BOTTOMALIGN | TPM_RIGHTALIGN) {
        Some(Choice::Action(action)) => {
            shared.chosen.borrow_mut().push_back((generation, action));
        }
        Some(Choice::Quit) => unsafe { PostQuitMessage(0) },
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_text_is_cut_to_its_field() {
        let mut field = [1u16; 4];
        copy_text(&mut field, "abcdef");
        assert_eq!(field, [97, 98, 99, 0]);
        copy_text(&mut field, "a");
        assert_eq!(field, [97, 0, 0, 0]);
    }
}
