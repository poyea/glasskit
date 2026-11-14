// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The keyboard, the text boxes, and the caret.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Input::Ime::{
    CFS_POINT, COMPOSITIONFORM, ImmGetContext, ImmReleaseContext, ImmSetCompositionWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_LEFT, VK_MENU, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCaretBlinkTime, KillTimer, SetTimer};

use super::state::State;
use super::text_box::Field;
use super::{Key, KeyEvent, WindowEvent};
use crate::clipboard;
use crate::edit::Editor;
use crate::widget::AnyId;
use crate::widget::tree::Content;

pub(super) const TIMER_BLINK: usize = 1;

/// The caret stops to blink after this time without input, as in Windows.
const BLINK_FOR: Duration = Duration::from_secs(5);

impl State {
    /// Handle a key. `repeat` is true for the messages that a held key sends again.
    pub(super) fn key(&self, hwnd: HWND, key: u16, repeat: bool) -> bool {
        if key == VK_ESCAPE.0 && self.is_dragging() {
            self.end_drag(hwnd, None);
            unsafe {
                let _ = ReleaseCapture();
            }
            return true;
        }
        let taken = self.widget_key(hwnd, key, repeat);
        if !taken {
            self.push_key(Key(key), repeat);
        }
        taken
    }

    /// Give a key that no widget used to the app.
    pub(super) fn push_key(&self, key: Key, repeat: bool) {
        if key.modifier() {
            return;
        }
        let down = |k: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(k.0)) } < 0;
        self.push(WindowEvent::Key(KeyEvent {
            key,
            ctrl: down(VK_CONTROL),
            shift: down(VK_SHIFT),
            alt: down(VK_MENU),
            repeat,
        }));
    }

    /// Handle a key in the focused widget, or Tab. It gives false when no widget used it.
    fn widget_key(&self, hwnd: HWND, key: u16, repeat: bool) -> bool {
        // A text box comes first, for all keys but Tab.
        if key != VK_TAB.0
            && let Some(index) = self.focused_box()
        {
            return self.box_key(hwnd, index, VIRTUAL_KEY(key), repeat);
        }
        match key {
            k if k == VK_TAB.0 => {
                let back = unsafe { GetKeyState(i32::from(VK_SHIFT.0)) } < 0;
                let next = self.tree.borrow().next_focus(self.now.get().focus, back);
                self.reveal_focus.set(true);
                self.set_interaction(hwnd, |now| {
                    now.focus = next;
                    now.cues = true;
                });
                true
            }
            k if (k == VK_SPACE.0 || k == VK_RETURN.0) && repeat => true,
            k if k == VK_SPACE.0 || k == VK_RETURN.0 => {
                let focus = self.now.get().focus;
                if let Some(focus) = focus.filter(|&f| self.tree.borrow().focusable(f)) {
                    self.set_interaction(hwnd, |now| now.cues = true);
                    self.click(hwnd, focus);
                }
                true
            }
            _ => false,
        }
    }

    /// The text box that has the focus, if one has it.
    fn focused_box(&self) -> Option<usize> {
        let focus = self.now.get().focus?;
        self.is_box(focus).then_some(focus)
    }

    pub(super) fn is_box(&self, index: usize) -> bool {
        let tree = self.tree.borrow();
        tree.focusable(index) && matches!(tree.nodes[index].content, Content::TextBox { .. })
    }

    /// The text of a text box, laid out where it shows.
    pub(super) fn field(&self, index: usize) -> Option<Field> {
        let tree = self.tree.borrow();
        let node = tree.get(index)?;
        let Content::TextBox { edit, scroll, .. } = &node.content else {
            return None;
        };
        Field::new(
            &self.formats,
            node.rect,
            edit.text(),
            edit.multiline(),
            *scroll,
        )
        .ok()
    }

    /// Change a text box with `change`, which gives whether the caret or the selection
    /// moved. Then draw again, and tell the app when the text changed.
    fn edit_box(
        &self,
        hwnd: HWND,
        index: usize,
        change: impl FnOnce(&mut Editor, Option<&Field>) -> bool,
    ) {
        let field = self.field(index);
        let (moved, changed) = {
            let mut tree = self.tree.borrow_mut();
            let Some(Content::TextBox { edit, .. }) = tree.get_mut(index).map(|n| &mut n.content)
            else {
                return;
            };
            let before = edit.revision();
            let moved = change(edit, field.as_ref());
            (moved, edit.revision() != before)
        };
        if moved || changed {
            self.reveal.set(true);
            self.invalidate(hwnd);
            self.restart_blink(hwnd);
        }
        if changed {
            self.push(WindowEvent::Changed(AnyId(index)));
        }
    }

    /// Read the editor of a text box.
    pub(super) fn read_box<R>(&self, index: usize, read: impl FnOnce(&Editor) -> R) -> Option<R> {
        match &self.tree.borrow().get(index)?.content {
            Content::TextBox { edit, .. } => Some(read(edit)),
            _ => None,
        }
    }

    /// Handle a key in a text box. It gives false for keys that the box does not take.
    fn box_key(&self, hwnd: HWND, index: usize, key: VIRTUAL_KEY, repeat: bool) -> bool {
        let down = |k: VIRTUAL_KEY| unsafe { GetKeyState(i32::from(k.0)) } < 0;
        let (ctrl, shift, alt) = (down(VK_CONTROL), down(VK_SHIFT), down(VK_MENU));
        let multiline = self.read_box(index, Editor::multiline).unwrap_or(false);
        // Move to a position from the layout: a line up or down, or a line edge.
        let to_line = |e: &mut Editor, f: Option<&Field>, at: &dyn Fn(&Field, u32) -> u32| {
            let Some(f) = f else {
                return false;
            };
            let to = e.byte_at(at(f, e.utf16_at(e.caret())));
            e.move_to(to, shift)
        };
        match key {
            VK_LEFT => self.edit_box(hwnd, index, |e, _| e.left(ctrl, shift)),
            VK_RIGHT => self.edit_box(hwnd, index, |e, _| e.right(ctrl, shift)),
            VK_UP | VK_DOWN if multiline => self.edit_box(hwnd, index, |e, f| {
                to_line(e, f, &|f, at| f.vertical(at, key == VK_DOWN))
            }),
            VK_HOME | VK_END if ctrl || !multiline => {
                let to = if key == VK_HOME { 0 } else { usize::MAX };
                self.edit_box(hwnd, index, |e, _| e.move_to(to, shift));
            }
            VK_HOME | VK_END => self.edit_box(hwnd, index, |e, f| {
                to_line(e, f, &|f, at| f.line_edge(at, key == VK_END))
            }),
            VK_BACK => self.edit_box(hwnd, index, |e, _| e.backspace(ctrl)),
            VK_DELETE => self.edit_box(hwnd, index, |e, _| e.delete(ctrl)),
            VK_RETURN if multiline => self.edit_box(hwnd, index, |e, _| e.insert("\n")),
            VK_RETURN => {
                if !repeat {
                    self.push(WindowEvent::Submitted(AnyId(index)));
                }
            }
            // The keys that type text stay in the box. AltGr comes as Ctrl and Alt.
            _ if !ctrl || alt => return !Key(key.0).types_no_text(),
            VIRTUAL_KEY(k) => match u8::try_from(k).map(char::from) {
                Ok('A') => self.edit_box(hwnd, index, |e, _| e.select_all()),
                Ok('C') => {
                    if let Some(text) = self.selected(index) {
                        clipboard::set_text(hwnd, &text);
                    }
                }
                Ok('X') => {
                    if let Some(text) = self.selected(index)
                        && clipboard::set_text(hwnd, &text)
                    {
                        self.edit_box(hwnd, index, |e, _| e.cut().is_some());
                    }
                }
                Ok('V') => {
                    if let Some(text) = clipboard::text(hwnd) {
                        self.edit_box(hwnd, index, |e, _| e.insert(&text));
                    }
                }
                Ok('Z') if shift => self.edit_box(hwnd, index, |e, _| e.redo()),
                Ok('Z') => self.edit_box(hwnd, index, |e, _| e.undo()),
                Ok('Y') => self.edit_box(hwnd, index, |e, _| e.redo()),
                _ => return false,
            },
        }
        true
    }

    /// The selected text of a text box, if some is selected.
    fn selected(&self, index: usize) -> Option<String> {
        self.read_box(index, |e| e.selected_text().to_string())
            .filter(|text| !text.is_empty())
    }

    /// Handle one `WM_CHAR`. It gives false when no text box has the focus.
    pub(super) fn type_char(&self, hwnd: HWND, unit: u16) -> bool {
        let Some(index) = self.focused_box() else {
            return false;
        };
        // Control characters come with keys that `box_key` handles.
        if unit >= 0x20 && unit != 0x7F {
            self.edit_box(hwnd, index, |e, _| e.type_unit(unit));
        }
        true
    }

    /// Put the caret of a text box at a point. With `extend`, select up to the point.
    pub(super) fn place_caret(&self, hwnd: HWND, index: usize, (x, y): (f32, f32), extend: bool) {
        self.edit_box(hwnd, index, |e, f| match f {
            Some(f) => {
                let to = e.byte_at(f.hit(x, y));
                e.move_to(to, extend)
            }
            None => false,
        });
    }

    /// Select the word, for a double click, or the line, for a triple click, at a point.
    pub(super) fn select_at(&self, hwnd: HWND, index: usize, (x, y): (f32, f32), clicks: u32) {
        self.edit_box(hwnd, index, |e, f| {
            let Some(f) = f else {
                return false;
            };
            let at = e.byte_at(f.hit(x, y));
            if clicks == 2 {
                e.select_word(at)
            } else {
                e.select_line(at)
            }
        });
    }

    /// Show the caret, and blink it if a text box has the focus in the active window.
    pub(super) fn restart_blink(&self, hwnd: HWND) {
        let mut now = self.now.get();
        if now.caret_off {
            now.caret_off = false;
            self.now.set(now);
            self.invalidate(hwnd);
        }
        // The user can turn blinking off: then the time is INFINITE.
        let period = unsafe { GetCaretBlinkTime() };
        let blink = now.active && self.focused_box().is_some() && period > 0 && period != u32::MAX;
        unsafe {
            if blink {
                self.blink_since.set(Instant::now());
                SetTimer(Some(hwnd), TIMER_BLINK, period, None);
            } else {
                let _ = KillTimer(Some(hwnd), TIMER_BLINK);
            }
        }
    }

    pub(super) fn blink(&self, hwnd: HWND) {
        let mut now = self.now.get();
        if self.blink_since.get().elapsed() >= BLINK_FOR {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_BLINK);
            }
            now.caret_off = false;
        } else {
            now.caret_off = !now.caret_off;
        }
        self.now.set(now);
        self.invalidate(hwnd);
    }

    /// Scroll the focused text box, so its caret shows.
    pub(super) fn reveal_caret(&self) {
        let Some(index) = self.focused_box() else {
            return;
        };
        let Some(field) = self.field(index) else {
            return;
        };
        if let Some(Content::TextBox { edit, scroll, .. }) = self
            .tree
            .borrow_mut()
            .get_mut(index)
            .map(|n| &mut n.content)
        {
            *scroll = field.reveal(edit.utf16_at(edit.caret()), *scroll);
        }
    }

    /// Show the text that an input method makes at the caret of the focused text box.
    pub(super) fn place_ime(&self, hwnd: HWND) {
        let Some(index) = self.focused_box() else {
            return;
        };
        let (Some(field), Some(at)) = (
            self.field(index),
            self.read_box(index, |e| e.utf16_at(e.caret())),
        ) else {
            return;
        };
        let (x, y, _) = field.caret(at);
        let s = self.scale.get();
        let form = COMPOSITIONFORM {
            dwStyle: CFS_POINT,
            ptCurrentPos: POINT {
                x: (x * s).round() as i32,
                y: (y * s).round() as i32,
            },
            rcArea: RECT::default(),
        };
        unsafe {
            let context = ImmGetContext(hwnd);
            if !context.is_invalid() {
                let _ = ImmSetCompositionWindow(context, &form);
                let _ = ImmReleaseContext(hwnd, context);
            }
        }
    }
}
