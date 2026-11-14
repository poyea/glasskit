// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A menu that opens at a point of the screen: the menu of a tray, or of a window.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, TRUE, WPARAM};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, PostQuitMessage, SetForegroundWindow,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TPM_TOPALIGN,
    TRACK_POPUP_MENU_FLAGS, TrackPopupMenu, WM_NULL,
};
use windows::core::{BOOL, HSTRING, PCSTR, PCWSTR, w};

/// The numbers of the uxtheme.dll exports for dark menus. They have no names.
const SET_PREFERRED_APP_MODE: u16 = 135;
const ALLOW_DARK_MODE_FOR_WINDOW: u16 = 133;
const FLUSH_MENU_THEMES: u16 = 136;
/// The app mode that follows the Windows light or dark setting.
const APP_MODE_ALLOW_DARK: i32 = 1;
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

/// A menu item as the window procedure keeps it. A command is an index into the command list.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
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

pub(crate) fn to_entries<C: Clone>(items: &[MenuItem<C>], commands: &mut Vec<C>) -> Vec<Entry> {
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

#[derive(Clone, Copy)]
pub(crate) enum Choice {
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

/// Show a menu with a corner at a point of the screen, and wait for the choice of the user.
pub(crate) fn track(
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

/// Make the menus follow the Windows dark mode, as File Explorer does. Windows has no public
/// API for this. uxtheme.dll exports the functions only by number. If a function is missing,
/// the menus stay light.
pub(crate) fn allow_dark_menus(hwnd: HWND) {
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
