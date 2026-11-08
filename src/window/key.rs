// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Keys that no widget used, for the shortcuts of the app.

/// A key on the keyboard, as its Windows virtual-key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(pub u16);

impl Key {
    /// Backspace.
    pub const BACKSPACE: Key = Key(0x08);
    /// Tab.
    pub const TAB: Key = Key(0x09);
    /// Enter.
    pub const ENTER: Key = Key(0x0D);
    /// Esc.
    pub const ESCAPE: Key = Key(0x1B);
    /// The space bar.
    pub const SPACE: Key = Key(0x20);
    /// Page Up.
    pub const PAGE_UP: Key = Key(0x21);
    /// Page Down.
    pub const PAGE_DOWN: Key = Key(0x22);
    /// End.
    pub const END: Key = Key(0x23);
    /// Home.
    pub const HOME: Key = Key(0x24);
    /// The left arrow.
    pub const LEFT: Key = Key(0x25);
    /// The up arrow.
    pub const UP: Key = Key(0x26);
    /// The right arrow.
    pub const RIGHT: Key = Key(0x27);
    /// The down arrow.
    pub const DOWN: Key = Key(0x28);
    /// Insert.
    pub const INSERT: Key = Key(0x2D);
    /// Delete.
    pub const DELETE: Key = Key(0x2E);

    /// The key of a letter or a digit on the main keys. Lower and upper case give the same
    /// key.
    ///
    /// # Panics
    ///
    /// If `c` is not an ASCII letter or digit.
    pub const fn char(c: char) -> Key {
        match c {
            'a'..='z' => Key(c as u16 - 0x20),
            'A'..='Z' | '0'..='9' => Key(c as u16),
            _ => panic!("Key::char takes an ASCII letter or digit"),
        }
    }

    /// The function key F`n`, from F1 to F24.
    ///
    /// # Panics
    ///
    /// If `n` is not from 1 to 24.
    pub const fn function(n: u8) -> Key {
        assert!(n >= 1 && n <= 24, "Key::function takes 1 to 24");
        Key(0x6F + n as u16)
    }

    /// Shift, Ctrl, Alt, or a Windows key, which alone give no event.
    pub(super) fn modifier(self) -> bool {
        matches!(self.0, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
    }

    /// A key that types no text, so a text box gives it to the app.
    pub(super) fn types_no_text(self) -> bool {
        self == Key::ESCAPE
            || self == Key::PAGE_UP
            || self == Key::PAGE_DOWN
            || (0x70..=0x87).contains(&self.0)
    }
}

/// A key press that no widget used. A text box uses the keys that type and edit text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyEvent {
    /// The key.
    pub key: Key,
    /// Ctrl was down.
    pub ctrl: bool,
    /// Shift was down.
    pub shift: bool,
    /// Alt was down.
    pub alt: bool,
    /// The key is held, and Windows sends it again.
    pub repeat: bool,
}

impl KeyEvent {
    /// Whether this is `key` with Ctrl, and without Alt or Shift: a shortcut such as Ctrl+S.
    pub fn is_ctrl(&self, key: Key) -> bool {
        self.key == key && self.ctrl && !self.alt && !self.shift
    }

    /// Whether this is `key` without Ctrl, Alt, or Shift.
    pub fn is_plain(&self, key: Key) -> bool {
        self.key == key && !self.ctrl && !self.alt && !self.shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_digits_are_their_codes() {
        assert_eq!(Key::char('s'), Key(0x53));
        assert_eq!(Key::char('S'), Key(0x53));
        assert_eq!(Key::char('7'), Key(0x37));
        assert_eq!(Key::function(1), Key(0x70));
        assert_eq!(Key::function(24), Key(0x87));
    }

    #[test]
    fn shortcuts_match_their_modifiers() {
        let save = KeyEvent {
            key: Key::char('s'),
            ctrl: true,
            shift: false,
            alt: false,
            repeat: false,
        };
        assert!(save.is_ctrl(Key::char('S')));
        assert!(!save.is_plain(Key::char('S')));
        assert!(Key(0x10).modifier());
        assert!(Key::function(5).types_no_text());
        assert!(!Key::char('a').types_no_text());
    }
}
