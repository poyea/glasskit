// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text and the caret of the input box of a popup. It has no window code, so it is
//! easy to test.

/// The most characters that the box takes.
const MAX_CHARS: usize = 200;

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
