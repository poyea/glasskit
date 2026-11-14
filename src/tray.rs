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

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, TRUE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIN_BALLOONUSERCLICK, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DefWindowProcW, DestroyIcon, DestroyMenu,
    GetCursorPos, GetSystemMetrics, HICON, HMENU, ICONINFO, IDC_ARROW, MENU_ITEM_FLAGS, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage,
    RegisterWindowMessageW, SM_CXSMICON, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TPM_TOPALIGN,
    TRACK_POPUP_MENU_FLAGS, TrackPopupMenu, WM_APP, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};
use windows::core::{BOOL, HSTRING, PCSTR, PCWSTR, w};

use crate::error::{Call, Error, Result};
use crate::events::{EventLoop, Source};
use crate::native::{self, NativeWindow};

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
/// One item of the menu of a tray, or of a menu of a window with [`Window::menu`]. `C` is
/// the command type of the app.
///
/// [`Window::menu`]: crate::window::Window::menu
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

/// The picture of a tray icon, or of a window with [`Window::set_icon`].
///
/// [`Window::set_icon`]: crate::window::Window::set_icon
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

/// An icon handle that Windows destroys when the value is dropped.
pub(crate) struct IconHandle(pub(crate) HICON);

impl IconHandle {
    pub(crate) fn new(icon: &Icon) -> Result<Self> {
        create_icon(icon).map(Self)
    }
}

impl Drop for IconHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyIcon(self.0);
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

/// Show a menu with a corner at a point of the screen, and wait for the choice of the user.
fn track(
    hwnd: HWND,
    entries: &[Entry],
    at: POINT,
    corner: TRACK_POPUP_MENU_FLAGS,
) -> Option<Choice> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut choices = Vec::new();
        append_entries(menu, entries, &mut choices);
        // Without this call, the menu does not close when the user clicks outside it.
        let _ = SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | corner,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        let index = (chosen.0 as usize).wrapping_sub(FIRST_ID);
        choices.get(index).copied()
    }
}

/// Show a menu of a window at a point of the screen, and give the command that the user
/// picked.
pub(crate) fn popup_menu<C: Clone>(hwnd: HWND, at: POINT, items: &[MenuItem<C>]) -> Option<C> {
    let mut commands = Vec::new();
    let entries = to_entries(items, &mut commands);
    allow_dark_menus(hwnd);
    match track(hwnd, &entries, at, TPM_LEFTALIGN | TPM_TOPALIGN)? {
        Choice::Action(index) => commands.get(index).cloned(),
        Choice::Quit => {
            unsafe { PostQuitMessage(0) };
            None
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
    fn a_long_text_is_cut_to_its_field() {
        let mut field = [1u16; 4];
        copy_text(&mut field, "abcdef");
        assert_eq!(field, [97, 98, 99, 0]);
        copy_text(&mut field, "a");
        assert_eq!(field, [97, 0, 0, 0]);
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
