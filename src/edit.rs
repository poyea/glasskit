// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text, caret, and selection of an input box, and the edits on them. It has no window
//! code: the window gives it characters and positions.

use std::ops::Range;

/// The most undo steps that an input box keeps.
const UNDO_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: usize,
}

/// Which edit came last, so that typed characters undo together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Last {
    Other,
    Typing,
}

/// The kinds of characters that a double click tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Other,
}

impl Class {
    fn of(c: char) -> Self {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    }
}

/// All positions are byte indices on character boundaries of the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Editor {
    text: String,
    caret: usize,
    /// The other end of the selection. It is equal to the caret when nothing is selected.
    anchor: usize,
    multiline: bool,
    max_chars: usize,
    /// The first half of a character that comes in two `WM_CHAR` messages.
    high: Option<u16>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Last,
    /// It goes up at each change of the text.
    revision: u64,
}

impl Editor {
    pub fn new(multiline: bool, max_chars: usize) -> Self {
        Self {
            text: String::new(),
            caret: 0,
            anchor: 0,
            multiline,
            max_chars,
            high: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last: Last::Other,
            revision: 0,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn multiline(&self) -> bool {
        self.multiline
    }

    pub fn set_multiline(&mut self, multiline: bool) {
        self.multiline = multiline;
    }

    pub fn set_max_chars(&mut self, max_chars: usize) {
        self.max_chars = max_chars;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }

    /// Replace all text, as the app does. The caret goes to the end, and undo starts again.
    pub fn set_text(&mut self, text: &str) {
        self.text = self.clean(text, self.max_chars);
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.undo.clear();
        self.redo.clear();
        self.last = Last::Other;
        self.revision += 1;
    }

    /// A position in UTF-16 units, as DirectWrite counts.
    pub fn utf16_at(&self, byte: usize) -> u32 {
        self.text[..byte.min(self.text.len())]
            .encode_utf16()
            .count() as u32
    }

    /// The byte index at a position in UTF-16 units. A position inside a character goes to
    /// its start.
    pub fn byte_at(&self, units: u32) -> usize {
        let mut count = 0;
        for (i, c) in self.text.char_indices() {
            count += c.len_utf16() as u32;
            if count > units {
                return i;
            }
        }
        self.text.len()
    }

    /// Add one UTF-16 unit that the user typed. A character outside the Basic Multilingual
    /// Plane comes as two units.
    pub fn type_unit(&mut self, unit: u16) -> bool {
        let units = match self.high.take() {
            Some(high) => vec![high, unit],
            None if (0xD800..0xDC00).contains(&unit) => {
                self.high = Some(unit);
                return false;
            }
            None => vec![unit],
        };
        let typed: String = char::decode_utf16(units).filter_map(|c| c.ok()).collect();
        self.replace(&typed, Last::Typing)
    }

    /// Add text at the caret, in place of the selection, as a paste does. A box with one
    /// line takes only the first line.
    pub fn insert(&mut self, text: &str) -> bool {
        self.replace(text, Last::Other)
    }

    pub fn backspace(&mut self, word: bool) -> bool {
        if self.caret == self.anchor {
            self.anchor = if word {
                self.word_left(self.caret)
            } else {
                self.char_left(self.caret)
            };
        }
        self.replace("", Last::Other)
    }

    pub fn delete(&mut self, word: bool) -> bool {
        if self.caret == self.anchor {
            self.anchor = if word {
                self.word_right(self.caret)
            } else {
                self.char_right(self.caret)
            };
        }
        self.replace("", Last::Other)
    }

    /// Remove the selection and give its text, for a cut.
    pub fn cut(&mut self) -> Option<String> {
        let text = self.selected_text().to_string();
        (!text.is_empty() && self.replace("", Last::Other)).then_some(text)
    }

    /// Move the caret one character, or one word, to the left. Without `extend`, a
    /// selection collapses to its left end.
    pub fn left(&mut self, word: bool, extend: bool) -> bool {
        let to = if self.caret != self.anchor && !extend {
            self.selection().start
        } else if word {
            self.word_left(self.caret)
        } else {
            self.char_left(self.caret)
        };
        self.move_to(to, extend)
    }

    pub fn right(&mut self, word: bool, extend: bool) -> bool {
        let to = if self.caret != self.anchor && !extend {
            self.selection().end
        } else if word {
            self.word_right(self.caret)
        } else {
            self.char_right(self.caret)
        };
        self.move_to(to, extend)
    }

    /// Put the caret at a byte index. With `extend`, the selection grows or shrinks to it.
    pub fn move_to(&mut self, to: usize, extend: bool) -> bool {
        let to = self.boundary(to);
        let before = (self.caret, self.anchor);
        self.caret = to;
        if !extend {
            self.anchor = to;
        }
        self.last = Last::Other;
        before != (self.caret, self.anchor)
    }

    pub fn select_all(&mut self) -> bool {
        let before = (self.caret, self.anchor);
        self.anchor = 0;
        self.caret = self.text.len();
        self.last = Last::Other;
        before != (self.caret, self.anchor)
    }

    /// Select the word at a byte index, as a double click does: the letters and digits
    /// around it, or the spaces, or the other characters.
    pub fn select_word(&mut self, at: usize) -> bool {
        let at = self.boundary(at);
        // At the end of a word, the word before counts.
        let Some(c) = self.text[at..]
            .chars()
            .next()
            .filter(|&c| !(c == '\n' && at > 0))
            .or_else(|| self.text[..at].chars().next_back())
        else {
            return false;
        };
        let class = Class::of(c);
        let start = self.text[..at]
            .char_indices()
            .rev()
            .find(|&(_, c)| Class::of(c) != class)
            .map_or(0, |(i, c)| i + c.len_utf8());
        let end = self.text[at..]
            .char_indices()
            .find(|&(_, c)| Class::of(c) != class)
            .map_or(self.text.len(), |(i, _)| at + i);
        self.select(start, end)
    }

    /// Select the line at a byte index, without its line break, as a triple click does.
    pub fn select_line(&mut self, at: usize) -> bool {
        let at = self.boundary(at);
        let start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[at..]
            .find('\n')
            .map_or(self.text.len(), |i| at + i);
        self.select(start, end)
    }

    fn select(&mut self, start: usize, end: usize) -> bool {
        let before = (self.caret, self.anchor);
        self.anchor = start;
        self.caret = end;
        self.last = Last::Other;
        before != (self.caret, self.anchor)
    }

    pub fn undo(&mut self) -> bool {
        self.step(true)
    }

    pub fn redo(&mut self) -> bool {
        self.step(false)
    }

    fn step(&mut self, back: bool) -> bool {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(s) = from.pop() else {
            return false;
        };
        to.push(Snapshot {
            text: std::mem::replace(&mut self.text, s.text),
            caret: self.caret,
            anchor: self.anchor,
        });
        self.caret = s.caret;
        self.anchor = s.anchor;
        self.last = Last::Other;
        self.revision += 1;
        true
    }

    /// Put `text` in place of the selection. It gives false, and changes nothing, when the
    /// text stays the same.
    fn replace(&mut self, text: &str, kind: Last) -> bool {
        let range = self.selection();
        let room = self
            .max_chars
            .saturating_sub(self.text.chars().count() - self.text[range.clone()].chars().count());
        let clean = self.clean(text, room);
        if clean.is_empty() && range.is_empty() {
            return false;
        }
        if !(kind == Last::Typing && self.last == Last::Typing && range.is_empty()) {
            self.undo.push(Snapshot {
                text: self.text.clone(),
                caret: self.caret,
                anchor: self.anchor,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last = kind;
        self.text.replace_range(range.clone(), &clean);
        self.caret = range.start + clean.len();
        self.anchor = self.caret;
        self.revision += 1;
        true
    }

    /// The part of `text` that the box takes: at most `room` characters, with line breaks
    /// as `\n` in a box with many lines, and no other control characters.
    fn clean(&self, text: &str, room: usize) -> String {
        let text = if self.multiline {
            text.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            text.split(['\r', '\n']).next().unwrap_or("").to_string()
        };
        text.chars()
            .filter(|&c| c == '\n' || !c.is_control())
            .take(room)
            .collect()
    }

    fn boundary(&self, mut i: usize) -> usize {
        i = i.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn char_left(&self, i: usize) -> usize {
        self.text[..i]
            .chars()
            .next_back()
            .map_or(i, |c| i - c.len_utf8())
    }

    fn char_right(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }

    /// The start of the word at the left: past spaces, then past the word.
    fn word_left(&self, i: usize) -> usize {
        let before = &self.text[..i];
        let trimmed = before.trim_end();
        trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(j, c)| j + c.len_utf8())
    }

    /// The start of the next word: past the word, then past spaces.
    fn word_right(&self, i: usize) -> usize {
        let after = &self.text[i..];
        let word = after.find(char::is_whitespace).unwrap_or(after.len());
        let rest = &after[word..];
        let spaces = rest.len() - rest.trim_start().len();
        i + word + spaces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str, multiline: bool) -> Editor {
        let mut e = Editor::new(multiline, usize::MAX);
        for unit in text.encode_utf16() {
            e.type_unit(unit);
        }
        e
    }

    #[test]
    fn typing_adds_at_the_caret() {
        let mut e = typed("ac", false);
        e.left(false, false);
        e.type_unit('b' as u16);
        assert_eq!(e.text(), "abc");
        assert_eq!(e.utf16_at(e.caret()), 2);
    }

    #[test]
    fn a_character_in_two_units_comes_together() {
        let e = typed("a😀憶", false);
        assert_eq!(e.text(), "a😀憶");
        assert_eq!(e.utf16_at(e.caret()), 4);
        assert_eq!(e.byte_at(3), 5);
        assert_eq!(e.byte_at(2), 1);
    }

    #[test]
    fn backspace_and_delete_remove_one_character() {
        let mut e = typed("x😀y", false);
        e.left(false, false);
        assert!(e.backspace(false));
        assert_eq!(e.text(), "xy");
        e.move_to(0, false);
        assert!(e.delete(false));
        assert_eq!(e.text(), "y");
        e.move_to(9, false);
        assert!(!e.delete(false));
        assert!(e.backspace(false));
        assert!(!e.backspace(false));
    }

    #[test]
    fn moves_stop_at_the_ends() {
        let mut e = typed("ab", false);
        assert!(!e.right(false, false));
        assert!(e.move_to(0, false));
        assert!(!e.left(false, false));
    }

    #[test]
    fn words_skip_spaces() {
        let mut e = typed("one two  three", false);
        e.left(true, false);
        assert_eq!(e.caret(), 9);
        e.left(true, false);
        assert_eq!(e.caret(), 4);
        e.right(true, false);
        assert_eq!(e.caret(), 9);
        e.move_to(e.text().len(), false);
        assert!(e.backspace(true));
        assert_eq!(e.text(), "one two  ");
    }

    #[test]
    fn typing_replaces_the_selection() {
        let mut e = typed("hello world", false);
        e.left(true, true);
        assert_eq!(e.selected_text(), "world");
        e.type_unit('X' as u16);
        assert_eq!(e.text(), "hello X");
        e.select_all();
        assert_eq!(e.cut().as_deref(), Some("hello X"));
        assert_eq!(e.text(), "");
        assert_eq!(e.cut(), None);
    }

    #[test]
    fn a_move_without_shift_collapses_the_selection() {
        let mut e = typed("abc", false);
        e.select_all();
        e.left(false, false);
        assert_eq!((e.caret(), e.selection()), (0, 0..0));
    }

    #[test]
    fn one_line_takes_the_first_line_of_a_paste() {
        let mut e = typed("", false);
        assert!(e.insert("one\ttwo\r\nthree"));
        assert_eq!(e.text(), "onetwo");
        assert!(!e.insert("\n"));
    }

    #[test]
    fn many_lines_keep_line_breaks_as_newlines() {
        let mut e = typed("", true);
        e.insert("a\r\nb\rc\n");
        assert_eq!(e.text(), "a\nb\nc\n");
    }

    #[test]
    fn the_box_has_a_limit() {
        let mut e = Editor::new(false, 5);
        e.insert(&"x".repeat(9));
        assert_eq!(e.text(), "xxxxx");
        assert!(!e.type_unit('y' as u16));
        e.select_all();
        assert!(e.type_unit('y' as u16));
        assert_eq!(e.text(), "y");
    }

    #[test]
    fn a_double_click_selects_the_word() {
        let mut e = typed("say hello_2 world!", false);
        e.select_word(6);
        assert_eq!(e.selected_text(), "hello_2");
        e.select_word(10);
        assert_eq!(e.selected_text(), "hello_2");
        e.select_word(3);
        assert_eq!(e.selected_text(), " ");
        e.select_word(17);
        assert_eq!(e.selected_text(), "!");
        e.select_word(99);
        assert_eq!(e.selected_text(), "!");
    }

    #[test]
    fn a_word_can_be_in_any_script() {
        let mut e = typed("記憶 test", false);
        e.select_word(0);
        assert_eq!(e.selected_text(), "記憶");
    }

    #[test]
    fn a_triple_click_selects_the_line() {
        let mut e = typed("", true);
        e.insert("one\ntwo two\nthree");
        e.select_line(6);
        assert_eq!(e.selected_text(), "two two");
        e.select_line(0);
        assert_eq!(e.selected_text(), "one");
        e.select_line(99);
        assert_eq!(e.selected_text(), "three");
    }

    #[test]
    fn typing_undoes_together_and_redoes() {
        let mut e = typed("ab", false);
        e.insert(" cd");
        e.type_unit('e' as u16);
        assert!(e.undo());
        assert_eq!(e.text(), "ab cd");
        assert!(e.undo());
        assert_eq!(e.text(), "ab");
        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(!e.undo());
        assert!(e.redo());
        assert!(e.redo());
        assert_eq!(e.text(), "ab cd");
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut e = typed("ab", false);
        e.undo();
        e.type_unit('z' as u16);
        assert!(!e.redo());
        assert_eq!(e.text(), "z");
    }

    #[test]
    fn set_text_resets_undo_and_counts_as_a_change() {
        let mut e = typed("ab", false);
        let r = e.revision();
        e.set_text("new\nline");
        assert_eq!(e.text(), "new");
        assert!(e.revision() > r);
        assert!(!e.undo());
    }
}
