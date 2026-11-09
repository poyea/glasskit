// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text formats of a window, the text sizes for the layout, and the drawing of each
//! widget. All sizes are in DIPs.

use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_WORD_WRAPPING_NO_WRAP,
    IDWriteFactory, IDWriteTextFormat,
};
use windows_numerics::Vector2;

use super::text_box;
use crate::error::{Call, Result};
use crate::render::{self, Graphics};
use crate::theme::Theme;
use crate::theme::rgba;
use crate::widget::TextStyle;
use crate::widget::tree::{Content, Measure, NAV_ITEM_PADDING, Rect, Tree};

const BUTTON_RADIUS: f32 = 4.0;
const CARD_RADIUS: f32 = 8.0;
/// The mark of a selected navigation item: a bar at its left edge, in DIPs.
const NAV_MARK: (f32, f32) = (3.0, 16.0);
const FOCUS_GAP: f32 = 3.0;
const FOCUS_WIDTH: f32 = 2.0;

pub(super) struct Formats {
    pub(super) dwrite: IDWriteFactory,
    /// One for each [`TextStyle`], in the order of [`TextStyle::ALL`].
    text: Vec<IDWriteTextFormat>,
    label: IDWriteTextFormat,
    /// Body text on one line, for a text box.
    pub(super) line: IDWriteTextFormat,
}

impl Formats {
    pub(super) fn new(gfx: &Graphics) -> Result<Self> {
        let text = TextStyle::ALL
            .iter()
            .map(|style| {
                let weight = if style.semibold() {
                    DWRITE_FONT_WEIGHT_SEMI_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                };
                gfx.text_format(style.size(), weight, false)
            })
            .collect::<Result<Vec<_>>>()?;
        let label = gfx.text_format(TextStyle::Body.size(), DWRITE_FONT_WEIGHT_NORMAL, true)?;
        let line = gfx.text_format(TextStyle::Body.size(), DWRITE_FONT_WEIGHT_NORMAL, false)?;
        for format in [&label, &line] {
            unsafe {
                format
                    .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                    .call("SetWordWrapping")?;
            }
        }
        Ok(Self {
            dwrite: gfx.dwrite(),
            text,
            label,
            line,
        })
    }

    pub(super) fn text_format(&self, style: TextStyle) -> &IDWriteTextFormat {
        let i = TextStyle::ALL.iter().position(|&s| s == style).unwrap_or(1);
        &self.text[i]
    }

    fn size_of(&self, text: &str, format: &IDWriteTextFormat, max_width: f32) -> (f32, f32) {
        render::text_size(&self.dwrite, text, format, max_width).unwrap_or((0.0, 0.0))
    }
}

impl Measure for Formats {
    fn text(&self, text: &str, style: TextStyle, max_width: f32) -> (f32, f32) {
        self.size_of(text, self.text_format(style), max_width)
    }

    fn label(&self, label: &str) -> (f32, f32) {
        // The label format centers its text, and centered text has no width in a box
        // without a limit. The line format has the same font, and starts at the left.
        self.size_of(label, &self.line, f32::MAX / 4.0)
    }
}

/// What the pointer and the keyboard do now.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Interaction {
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    pub focus: Option<usize>,
    /// Show the focus ring. It shows after the user uses the keyboard, as in Windows.
    pub cues: bool,
    /// The window has the keyboard focus, so the caret of a text box shows.
    pub active: bool,
    /// The caret is in the off half of a blink.
    pub caret_off: bool,
}

pub(super) fn paint(
    dc: &ID2D1DeviceContext,
    tree: &Tree,
    theme: &Theme,
    formats: &Formats,
    now: Interaction,
) -> windows::core::Result<()> {
    unsafe {
        dc.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    }
    for index in tree.draw_order() {
        let node = &tree.nodes[index];
        let r = node.rect;
        match &node.content {
            Content::Blank => {}
            Content::Card => panel(dc, r, CARD_RADIUS, theme.card, theme.card_border)?,
            Content::Text { text, style } => {
                let color = if node.enabled {
                    theme.text
                } else {
                    theme.subtle_text
                };
                draw_text(dc, formats, text, formats.text_format(*style), r, color)?;
            }
            Content::TextBox { .. } => text_box::draw(dc, theme, formats, node, index, now)?,
            Content::NavItem { label, selected } => {
                let hover = now.hover == Some(index);
                let pressed = now.pressed == Some(index) && hover;
                let fill = match (*selected, hover, pressed) {
                    (_, _, true) => Some(theme.button_pressed),
                    (true, true, _) => Some(theme.button_hover),
                    (true, false, _) | (false, true, _) => Some(theme.button),
                    (false, false, _) => None,
                };
                if let Some(fill) = fill {
                    panel(dc, r, BUTTON_RADIUS, fill, rgba(0.0, 0.0, 0.0, 0.0))?;
                }
                if *selected {
                    let (w, h) = NAV_MARK;
                    let mark = Rect::new(r.x, r.y + (r.h - h) / 2.0, w, h);
                    panel(dc, mark, w / 2.0, theme.accent, theme.accent)?;
                }
                let color = if node.enabled {
                    theme.text
                } else {
                    theme.subtle_text
                };
                let (_, text_h) = formats.label(label);
                let (left, right) = NAV_ITEM_PADDING;
                let text = Rect::new(
                    r.x + left,
                    r.y + (r.h - text_h) / 2.0,
                    r.w - left - right,
                    text_h,
                );
                draw_text(dc, formats, label, &formats.line, text, color)?;
                if now.cues && now.focus == Some(index) {
                    focus_ring(dc, theme, r)?;
                }
            }
            Content::Button { label, accent } => {
                let state = ButtonState {
                    hover: now.hover == Some(index),
                    pressed: now.pressed == Some(index) && now.hover == Some(index),
                    enabled: node.enabled,
                };
                button(dc, theme, r, *accent, state)?;
                let color = match (*accent, node.enabled) {
                    (_, false) => theme.subtle_text,
                    (true, true) => theme.on_accent,
                    (false, true) => theme.text,
                };
                draw_text(dc, formats, label, &formats.label, r, color)?;
                if now.cues && now.focus == Some(index) {
                    focus_ring(dc, theme, r)?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ButtonState {
    hover: bool,
    pressed: bool,
    enabled: bool,
}

fn button(
    dc: &ID2D1DeviceContext,
    theme: &Theme,
    r: Rect,
    accent: bool,
    s: ButtonState,
) -> windows::core::Result<()> {
    let mut fill = if accent {
        let mut c = theme.accent;
        if s.pressed {
            c.a *= 0.8;
        } else if s.hover {
            c.a *= 0.9;
        }
        c
    } else if s.pressed {
        theme.button_pressed
    } else if s.hover {
        theme.button_hover
    } else {
        theme.button
    };
    if !s.enabled {
        fill.a *= 0.4;
    }
    panel(dc, r, BUTTON_RADIUS, fill, theme.button_border)
}

fn panel(
    dc: &ID2D1DeviceContext,
    r: Rect,
    radius: f32,
    fill: D2D1_COLOR_F,
    border: D2D1_COLOR_F,
) -> windows::core::Result<()> {
    render::panel(dc, d2d(r), radius, &fill, &border)
}

fn focus_ring(dc: &ID2D1DeviceContext, theme: &Theme, r: Rect) -> windows::core::Result<()> {
    unsafe {
        let ring = render::rounded(d2d(r), -FOCUS_GAP, BUTTON_RADIUS + FOCUS_GAP);
        dc.DrawRoundedRectangle(
            &ring,
            &dc.CreateSolidColorBrush(&theme.text, None)?,
            FOCUS_WIDTH,
            None,
        );
    }
    Ok(())
}

fn draw_text(
    dc: &ID2D1DeviceContext,
    formats: &Formats,
    text: &str,
    format: &IDWriteTextFormat,
    r: Rect,
    color: D2D1_COLOR_F,
) -> windows::core::Result<()> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let layout = formats
            .dwrite
            .CreateTextLayout(&wide, format, r.w.max(1.0), r.h.max(1.0))?;
        dc.DrawTextLayout(
            Vector2 { X: r.x, Y: r.y },
            &layout,
            &dc.CreateSolidColorBrush(&color, None)?,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
        );
    }
    Ok(())
}

pub(super) fn d2d(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_has_the_width_of_its_text() {
        let _ui = crate::Ui::new().unwrap();
        let formats = Formats::new(&Graphics::new().unwrap()).unwrap();
        let (short, _) = formats.label("OK");
        let (long, height) = formats.label("Open the folder");
        assert!(short > 0.0 && height > 0.0);
        assert!(long > 3.0 * short, "{long} and {short}");
    }
}
