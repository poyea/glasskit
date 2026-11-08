// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The input box of a popup: its keys and its drawing.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::DWRITE_HIT_TEST_METRICS;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RIGHT,
};
use windows_numerics::Vector2;

use super::layout::Fonts;
use crate::edit::Editor;
use crate::render;
use crate::theme::Theme;

/// The most characters that the box takes.
const MAX_CHARS: usize = 200;

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
    edit: Editor,
    pub(super) focused: bool,
}

impl TextBox {
    pub(super) fn new(placeholder: String) -> Self {
        Self {
            placeholder,
            edit: Editor::new(false, MAX_CHARS),
            focused: false,
        }
    }

    pub(super) fn text(&self) -> &str {
        self.edit.text()
    }

    /// Add one UTF-16 unit that the user typed.
    pub(super) fn type_unit(&mut self, unit: u16) -> bool {
        self.edit.type_unit(unit)
    }

    /// Add text from the clipboard. It stops at the first line break.
    pub(super) fn insert(&mut self, text: &str) -> bool {
        self.edit.insert(text)
    }

    pub(super) fn backspace(&mut self) -> bool {
        self.edit.backspace(false)
    }

    /// Handle a key that moves the caret or deletes. It gives `None` for other keys, and
    /// else whether the text or the caret changed.
    pub(super) fn press(&mut self, key: u16) -> Option<bool> {
        let e = &mut self.edit;
        Some(match VIRTUAL_KEY(key) {
            VK_LEFT => e.left(false, false),
            VK_RIGHT => e.right(false, false),
            VK_HOME => e.move_to(0, false),
            VK_END => e.move_to(usize::MAX, false),
            VK_DELETE => e.delete(false),
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
            let edge = if self.focused {
                t.text
            } else {
                t.button_border
            };
            render::panel(dc, rect, radius, &t.button, &edge)?;

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
            let position = if empty {
                0
            } else {
                self.edit.utf16_at(self.edit.caret())
            };
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
