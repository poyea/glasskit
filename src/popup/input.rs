// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The input box of a popup: its text and caret, its keys, its drawing, and paste.

use windows::Win32::Foundation::{HGLOBAL, HWND};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT, ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS;
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT,
};
use windows_numerics::Vector2;

use super::layout::Fonts;
use crate::theme::Theme;

/// The most characters that the box takes.
const MAX_CHARS: usize = 200;
/// The clipboard text that a paste reads, at most, in UTF-16 units.
const MAX_PASTE: usize = 4096;

/// Where the input box shows.
#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub(super) rect: D2D_RECT_F,
    pub(super) radius: f32,
    /// The space between the edge and the text.
    pub(super) padding: f32,
}

pub(super) struct TextBox {
    pub(super) placeholder: String,
    text: String,
    /// A byte index on a character boundary of `text`.
    caret: usize,
    /// The first half of a character that comes in two `WM_CHAR` messages.
    high: Option<u16>,
    pub(super) focused: bool,
}

impl TextBox {
    pub(super) fn new(placeholder: String) -> Self {
        Self {
            placeholder,
            text: String::new(),
            caret: 0,
            high: None,
            focused: false,
        }
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The caret position in UTF-16 units, as DirectWrite counts.
    pub(super) fn caret_utf16(&self) -> u32 {
        self.text[..self.caret].encode_utf16().count() as u32
    }

    /// Add one UTF-16 unit that the user typed. A character outside the Basic Multilingual
    /// Plane comes as two units.
    pub(super) fn type_unit(&mut self, unit: u16) -> bool {
        let units = match self.high.take() {
            Some(high) => vec![high, unit],
            None if (0xD800..0xDC00).contains(&unit) => {
                self.high = Some(unit);
                return false;
            }
            None => vec![unit],
        };
        let typed: String = char::decode_utf16(units).filter_map(|c| c.ok()).collect();
        self.insert(&typed)
    }

    /// Add text, for example from the clipboard. It stops at the first line break and drops
    /// other control characters.
    pub(super) fn insert(&mut self, text: &str) -> bool {
        let line = text.split(['\r', '\n']).next().unwrap_or("");
        let room = MAX_CHARS.saturating_sub(self.text.chars().count());
        let clean: String = line
            .chars()
            .filter(|c| !c.is_control())
            .take(room)
            .collect();
        if clean.is_empty() {
            return false;
        }
        self.text.insert_str(self.caret, &clean);
        self.caret += clean.len();
        true
    }

    pub(super) fn backspace(&mut self) -> bool {
        match self.text[..self.caret].chars().next_back() {
            Some(c) => {
                self.caret -= c.len_utf8();
                self.text.remove(self.caret);
                true
            }
            None => false,
        }
    }

    pub(super) fn delete(&mut self) -> bool {
        if self.caret < self.text.len() {
            self.text.remove(self.caret);
            true
        } else {
            false
        }
    }

    pub(super) fn left(&mut self) -> bool {
        match self.text[..self.caret].chars().next_back() {
            Some(c) => {
                self.caret -= c.len_utf8();
                true
            }
            None => false,
        }
    }

    pub(super) fn right(&mut self) -> bool {
        match self.text[self.caret..].chars().next() {
            Some(c) => {
                self.caret += c.len_utf8();
                true
            }
            None => false,
        }
    }

    pub(super) fn home(&mut self) -> bool {
        std::mem::replace(&mut self.caret, 0) != 0
    }

    pub(super) fn end(&mut self) -> bool {
        std::mem::replace(&mut self.caret, self.text.len()) != self.text.len()
    }

    /// Handle a key that moves the caret or deletes. It gives `None` for other keys, and
    /// else whether the text or the caret changed.
    pub(super) fn press(&mut self, key: u16) -> Option<bool> {
        Some(match VIRTUAL_KEY(key) {
            VK_LEFT => self.left(),
            VK_RIGHT => self.right(),
            VK_HOME => self.home(),
            VK_END => self.end(),
            VK_DELETE => self.delete(),
            _ => return None,
        })
    }

    /// Draw the input box: its frame, the text or the placeholder, and the caret.
    pub(super) fn draw(
        &self,
        dc: &ID2D1DeviceContext,
        frame: Frame,
        theme: &Theme,
        fonts: &Fonts,
    ) -> windows::core::Result<()> {
        let Frame {
            rect,
            radius,
            padding,
        } = frame;
        let t = theme;
        unsafe {
            let shape = D2D1_ROUNDED_RECT {
                rect,
                radiusX: radius,
                radiusY: radius,
            };
            dc.FillRoundedRectangle(&shape, &dc.CreateSolidColorBrush(&t.button, None)?);
            let edge = if self.focused {
                t.text
            } else {
                t.button_border
            };
            let outline = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: rect.left + 0.5,
                    top: rect.top + 0.5,
                    right: rect.right - 0.5,
                    bottom: rect.bottom - 0.5,
                },
                radiusX: radius,
                radiusY: radius,
            };
            dc.DrawRoundedRectangle(&outline, &dc.CreateSolidColorBrush(&edge, None)?, 1.0, None);

            let empty = self.text().is_empty();
            let shown = if empty {
                self.placeholder.as_str()
            } else {
                self.text()
            };
            let wide: Vec<u16> = shown.encode_utf16().collect();
            let width = rect.right - rect.left - 2.0 * padding;
            let height = rect.bottom - rect.top;
            let layout = fonts
                .dwrite
                .CreateTextLayout(&wide, &fonts.input, width, height)?;
            // Where the caret is, so the text can scroll left to keep it in the box.
            let (mut x, mut y) = (0.0, 0.0);
            let mut caret = DWRITE_HIT_TEST_METRICS::default();
            let position = if empty { 0 } else { self.caret_utf16() };
            layout.HitTestTextPosition(position, false, &mut x, &mut y, &mut caret)?;
            let scroll = (x - width).max(0.0);
            let origin = Vector2 {
                X: rect.left + padding - scroll,
                Y: rect.top,
            };
            let mut color = t.text;
            if empty {
                color.a *= 0.5;
            }
            dc.PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_ALIASED);
            dc.DrawTextLayout(
                origin,
                &layout,
                &dc.CreateSolidColorBrush(&color, None)?,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            if self.focused {
                let x = (origin.X + x).round() + 0.5;
                let top = origin.Y + caret.top;
                dc.DrawLine(
                    Vector2 { X: x, Y: top },
                    Vector2 {
                        X: x,
                        Y: top + caret.height,
                    },
                    &dc.CreateSolidColorBrush(&t.text, None)?,
                    1.0,
                    None,
                );
            }
            dc.PopAxisAlignedClip();
        }
        Ok(())
    }
}

/// The text on the clipboard, if there is text.
pub(super) fn clipboard_text(hwnd: HWND) -> Option<String> {
    unsafe {
        OpenClipboard(Some(hwnd)).ok()?;
        let text = read_clipboard_text();
        let _ = CloseClipboard();
        text
    }
}

/// Read the text of the clipboard. The clipboard must be open.
unsafe fn read_clipboard_text() -> Option<String> {
    unsafe {
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
        let global = HGLOBAL(handle.0);
        let start = GlobalLock(global) as *const u16;
        if start.is_null() {
            return None;
        }
        let mut len = 0;
        while len < MAX_PASTE && *start.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(start, len));
        let _ = GlobalUnlock(global);
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> TextBox {
        let mut b = TextBox::new(String::new());
        for unit in text.encode_utf16() {
            b.type_unit(unit);
        }
        b
    }

    #[test]
    fn typing_adds_at_the_caret() {
        let mut b = typed("ac");
        b.left();
        b.type_unit('b' as u16);
        assert_eq!(b.text(), "abc");
        assert_eq!(b.caret_utf16(), 2);
    }

    #[test]
    fn a_character_in_two_units_comes_together() {
        let b = typed("a😀憶");
        assert_eq!(b.text(), "a😀憶");
        assert_eq!(b.caret_utf16(), 4);
    }

    #[test]
    fn backspace_and_delete_remove_one_character() {
        let mut b = typed("x😀y");
        b.left();
        assert!(b.backspace());
        assert_eq!(b.text(), "xy");
        b.home();
        assert!(b.delete());
        assert_eq!(b.text(), "y");
        b.end();
        assert!(!b.delete());
        assert!(b.backspace());
        assert!(!b.backspace());
    }

    #[test]
    fn moves_stop_at_the_ends() {
        let mut b = typed("ab");
        assert!(!b.right());
        assert!(b.home());
        assert!(!b.left());
        assert!(!b.home());
        assert!(b.end());
        assert!(!b.end());
    }

    #[test]
    fn paste_takes_one_line_without_control_characters() {
        let mut b = typed("");
        assert!(b.insert("one\ttwo\r\nthree"));
        assert_eq!(b.text(), "onetwo");
        assert!(!b.insert("\n"));
    }

    #[test]
    fn the_box_has_a_limit() {
        let mut b = typed("");
        b.insert(&"x".repeat(250));
        assert_eq!(b.text().chars().count(), MAX_CHARS);
        assert!(!b.type_unit('y' as u16));
    }
}
