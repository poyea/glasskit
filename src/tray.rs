// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A notification-area (tray) icon with a menu.
//!
//! A tray belongs to an [`EventLoop`]: when the user picks a menu command,
//! [`EventLoop::next`](crate::events::EventLoop::next) gives
//! [`Event::Command`](crate::events::Event::Command).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;
use std::rc::{Rc, Weak};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DefWindowProcW, DestroyIcon, DestroyMenu,
    GetCursorPos, GetSystemMetrics, HICON, HMENU, ICONINFO, IDC_ARROW, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage,
    RegisterWindowMessageW, SM_CXSMICON, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP,
};
use windows::core::{BOOL, HSTRING, PCSTR, PCWSTR, w};

use crate::error::{Call, Error, Result};
use crate::events::{EventLoop, Source};
use crate::window::{self, Window};

const CLASS_NAME: PCWSTR = w!("GlasskitTray");
/// The numbers of the uxtheme.dll exports for dark menus. They have no names.
const SET_PREFERRED_APP_MODE: u16 = 135;
const ALLOW_DARK_MODE_FOR_WINDOW: u16 = 133;
const FLUSH_MENU_THEMES: u16 = 136;
/// The app mode that follows the Windows light or dark setting.
const APP_MODE_ALLOW_DARK: i32 = 1;
const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
/// Menu item ids start here. 0 means that the user closed the menu without a choice.
const FIRST_ID: usize = 1;
/// One item of the tray menu. `C` is the command type of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem<C> {
    /// An item that the user can pick. The event loop then gives [`Event::Command`](crate::events::Event::Command).
    Command {
        /// The command that the event loop gives.
        command: C,
        /// The text of the item.
        label: String,
        /// A disabled item shows in gray, and the user cannot pick it.
        enabled: bool,
        /// A checked item shows a check mark.
        checked: bool,
    },
    /// A gray text that the user cannot pick, for example a status line.
    Label(String),
    /// A line between groups of items.
    Separator,
    /// An item that opens more items.
    Submenu {
        /// The text of the item.
        label: String,
        /// The items of the submenu.
        items: Vec<MenuItem<C>>,
    },
    /// The item that ends the app. The event loop then gives [`Event::Quit`](crate::events::Event::Quit).
    Quit(String),
}

impl<C> MenuItem<C> {
    /// An enabled, unchecked command item.
    pub fn command(command: C, label: impl Into<String>) -> Self {
        MenuItem::Command {
            command,
            label: label.into(),
            enabled: true,
            checked: false,
        }
    }

    /// Enable or disable a command item. Other items do not change.
    #[must_use]
    pub fn enabled(mut self, on: bool) -> Self {
        if let MenuItem::Command { enabled, .. } = &mut self {
            *enabled = on;
        }
        self
    }

    /// Set or clear the check mark of a command item. Other items do not change.
    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        if let MenuItem::Command { checked, .. } = &mut self {
            *checked = on;
        }
        self
    }
}

/// The picture of a tray icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    size: u32,
    pixels: Vec<u32>,
}

impl Icon {
    /// The size in pixels that Windows uses for tray icons now. It changes with the display
    /// scale, so make the icon at this size just before [`Tray::new`].
    pub fn tray_size() -> u32 {
        unsafe { GetSystemMetrics(SM_CXSMICON).max(16) as u32 }
    }

    /// A square icon of `size` by `size` pixels. `pixels` go row by row from the top-left
    /// corner. Each pixel is `0xAARRGGBB` with straight (not premultiplied) alpha.
    pub fn from_argb(size: u32, pixels: Vec<u32>) -> Result<Self> {
        if pixels.len() as u64 != u64::from(size) * u64::from(size) {
            return Err(Error::InvalidIcon {
                size,
                pixels: pixels.len(),
            });
        }
        Ok(Self { size, pixels })
    }

    /// The width and height in pixels.
    pub fn size(&self) -> u32 {
        self.size
    }
}

/// A menu item as the window procedure keeps it. A command is an index into the command list.
#[derive(Debug, Clone)]
enum Entry {
    Action {
        index: usize,
        label: String,
        enabled: bool,
        checked: bool,
    },
    Label(String),
    Separator,
    Submenu {
        label: String,
        entries: Vec<Entry>,
    },
    Quit(String),
}

/// The state of the window procedure. It has no type parameter, so one window class serves
/// all command types.
struct Shared {
    entries: RefCell<Vec<Entry>>,
    /// It changes with each new menu. A choice from an older menu is not used.
    generation: Cell<u64>,
    chosen: RefCell<VecDeque<(u64, usize)>>,
    icon_data: RefCell<NOTIFYICONDATAW>,
    taskbar_created: u32,
}

/// What the event loop asks for commands: the window state and the commands of the menu.
struct Core<C> {
    shared: Rc<Shared>,
    commands: RefCell<Vec<C>>,
}

impl<C: Clone> Source<C> for Core<C> {
    fn take_command(&self) -> Option<C> {
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

/// An icon handle that Windows destroys when the value is dropped.
struct IconHandle(HICON);

impl Drop for IconHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// A tray icon and its menu. `C` is the command type of the app, for example an enum.
///
/// It gives its commands to the [`EventLoop`] that it was made with.
pub struct Tray<C> {
    window: Window<Shared>,
    _icon: IconHandle,
    core: Rc<Core<C>>,
}

impl<C: Clone + 'static> Tray<C> {
    /// Add an icon with this tooltip to the notification area. The menu is empty until
    /// [`Tray::set_menu`].
    pub fn new(events: &mut EventLoop<C>, tooltip: &str, icon: &Icon) -> Result<Self> {
        let hinstance = events.ui().hinstance();
        window::register_class(CLASS_NAME, Some(wndproc), hinstance, IDC_ARROW)?;
        let icon = IconHandle(create_icon(icon)?);
        // Use a hidden top-level window. A message-only window does not get the
        // TaskbarCreated message when Explorer restarts.
        let hwnd = window::create_hidden(CLASS_NAME, hinstance)?;
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
            icon_data: RefCell::new(data),
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        });
        let window = Window::attach(hwnd, Rc::clone(&shared));
        allow_dark_menus(hwnd);

        let core = Rc::new(Core {
            shared,
            commands: RefCell::new(Vec::new()),
        });
        let source: Weak<dyn Source<C>> = Rc::downgrade(&core) as Weak<dyn Source<C>>;
        events.add_source(source);
        let tray = Self {
            window,
            _icon: icon,
            core,
        };
        let added = unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
        if !added.as_bool() {
            return Err(Error::TrayIconNotAdded);
        }
        Ok(tray)
    }

    fn shared(&self) -> &Shared {
        self.window.state()
    }

    /// Replace the menu. A choice that the user made in the old menu and that the event loop
    /// did not give yet is dropped.
    pub fn set_menu(&mut self, items: &[MenuItem<C>]) {
        let mut commands = Vec::new();
        let entries = to_entries(items, &mut commands);
        self.core.commands.replace(commands);
        let shared = self.shared();
        shared.entries.replace(entries);
        shared.generation.set(shared.generation.get() + 1);
    }

    /// Change the text that shows when the pointer is on the icon. Windows shows up to 127
    /// characters.
    pub fn set_tooltip(&self, tooltip: &str) {
        let mut data = self.shared().icon_data.borrow_mut();
        set_tip(&mut data, tooltip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &*data);
        }
    }
}

impl<C> Drop for Tray<C> {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &*self.window.state().icon_data.borrow());
        }
    }
}

fn to_entries<C: Clone>(items: &[MenuItem<C>], commands: &mut Vec<C>) -> Vec<Entry> {
    items
        .iter()
        .map(|item| match item {
            MenuItem::Command {
                command,
                label,
                enabled,
                checked,
            } => {
                commands.push(command.clone());
                Entry::Action {
                    index: commands.len() - 1,
                    label: label.clone(),
                    enabled: *enabled,
                    checked: *checked,
                }
            }
            MenuItem::Label(label) => Entry::Label(label.clone()),
            MenuItem::Separator => Entry::Separator,
            MenuItem::Submenu { label, items } => Entry::Submenu {
                label: label.clone(),
                entries: to_entries(items, commands),
            },
            MenuItem::Quit(label) => Entry::Quit(label.clone()),
        })
        .collect()
}

fn set_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    let max = data.szTip.len() - 1;
    let wide: Vec<u16> = tip.encode_utf16().take(max).collect();
    data.szTip = [0; 128];
    data.szTip[..wide.len()].copy_from_slice(&wide);
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let Some(shared) = window::state_of::<Shared>(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        match msg {
            WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                if event == WM_LBUTTONUP || event == WM_RBUTTONUP {
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

#[derive(Clone, Copy)]
enum Choice {
    Action(usize),
    Quit,
}

fn add_choice(choices: &mut Vec<Choice>, choice: Choice) -> usize {
    choices.push(choice);
    FIRST_ID + choices.len() - 1
}

fn append_entries(menu: HMENU, entries: &[Entry], choices: &mut Vec<Choice>) {
    let flag = |on: bool, f: MENU_ITEM_FLAGS| if on { f } else { MENU_ITEM_FLAGS(0) };
    unsafe {
        for entry in entries {
            match entry {
                Entry::Action {
                    index,
                    label,
                    enabled,
                    checked,
                } => {
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING | flag(!enabled, MF_GRAYED) | flag(*checked, MF_CHECKED),
                        add_choice(choices, Choice::Action(*index)),
                        &HSTRING::from(label),
                    );
                }
                Entry::Label(label) => {
                    let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, &HSTRING::from(label));
                }
                Entry::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
                Entry::Submenu { label, entries } => {
                    if let Ok(sub) = CreatePopupMenu() {
                        append_entries(sub, entries, choices);
                        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, &HSTRING::from(label));
                    }
                }
                Entry::Quit(label) => {
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        add_choice(choices, Choice::Quit),
                        &HSTRING::from(label),
                    );
                }
            }
        }
    }
}

fn show_menu(hwnd: HWND, shared: &Shared) {
    let entries = shared.entries.borrow().clone();
    let generation = shared.generation.get();
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let mut choices = Vec::new();
        append_entries(menu, &entries, &mut choices);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Without this call, the menu does not close when the user clicks outside it.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        let index = (chosen.0 as usize).wrapping_sub(FIRST_ID);
        match choices.get(index) {
            Some(Choice::Action(action)) => {
                shared.chosen.borrow_mut().push_back((generation, *action));
            }
            Some(Choice::Quit) => PostQuitMessage(0),
            None => {}
        }
    }
}

fn create_icon(image: &Icon) -> Result<HICON> {
    let size = image.size as i32;
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .call("CreateDIBSection")?;
        std::slice::from_raw_parts_mut(bits as *mut u32, image.pixels.len())
            .copy_from_slice(&image.pixels);

        let stride = ((size + 15) / 16 * 2) as usize;
        let mask_bits = vec![0u8; stride * size as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon.call("CreateIconIndirect")
    }
}

/// Make the menus follow the Windows dark mode, as File Explorer does. Windows has no public
/// API for this. uxtheme.dll exports the functions only by number. If a function is missing,
/// the menus stay light.
fn allow_dark_menus(hwnd: HWND) {
    unsafe {
        let Ok(uxtheme) = LoadLibraryW(w!("uxtheme.dll")) else {
            return;
        };
        let export = |number: u16| GetProcAddress(uxtheme, PCSTR(number as usize as *const u8));
        if let Some(f) = export(SET_PREFERRED_APP_MODE) {
            let set: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set(APP_MODE_ALLOW_DARK);
        }
        if let Some(f) = export(ALLOW_DARK_MODE_FOR_WINDOW) {
            let allow: unsafe extern "system" fn(HWND, BOOL) -> BOOL = std::mem::transmute(f);
            let _ = allow(hwnd, TRUE);
        }
        if let Some(f) = export(FLUSH_MENU_THEMES) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(f);
            flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_needs_one_pixel_per_cell() {
        assert!(Icon::from_argb(2, vec![0; 4]).is_ok());
        assert!(matches!(
            Icon::from_argb(2, vec![0; 3]),
            Err(Error::InvalidIcon { size: 2, pixels: 3 })
        ));
    }

    #[test]
    fn builders_change_only_command_items() {
        let item = MenuItem::command('a', "A").enabled(false).checked(true);
        assert_eq!(
            item,
            MenuItem::Command {
                command: 'a',
                label: "A".into(),
                enabled: false,
                checked: true,
            }
        );
        assert_eq!(
            MenuItem::<char>::Separator.checked(true),
            MenuItem::Separator
        );
    }

    #[test]
    fn entries_number_the_commands_in_order() {
        let items = vec![
            MenuItem::Label("status".into()),
            MenuItem::command(10, "a"),
            MenuItem::Submenu {
                label: "more".into(),
                items: vec![MenuItem::command(20, "b"), MenuItem::Separator],
            },
            MenuItem::Quit("quit".into()),
        ];
        let mut commands = Vec::new();
        let entries = to_entries(&items, &mut commands);
        assert_eq!(commands, [10, 20]);
        assert!(matches!(entries[1], Entry::Action { index: 0, .. }));
        let Entry::Submenu { entries: sub, .. } = &entries[2] else {
            panic!("expected a submenu");
        };
        assert!(matches!(sub[0], Entry::Action { index: 1, .. }));
        assert!(matches!(entries[3], Entry::Quit(_)));
    }
}
