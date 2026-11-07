// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text formats of a window, the text sizes for the layout, and the drawing of each
//! widget. All sizes are in DIPs.

use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_METRICS,
    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteTextFormat,
};
use windows_numerics::Vector2;

use crate::error::{Call, Result};
use crate::render::Graphics;
use crate::theme::Theme;
use crate::widget::TextStyle;
use crate::widget::tree::{Content, Measure, Rect, Tree};

const BUTTON_RADIUS: f32 = 4.0;
const CARD_RADIUS: f32 = 8.0;
const FOCUS_GAP: f32 = 3.0;
const FOCUS_WIDTH: f32 = 2.0;

pub(super) struct Formats {
    dwrite: IDWriteFactory,
    /// One for each [`TextStyle`], in the order of [`TextStyle::ALL`].
    text: Vec<IDWriteTextFormat>,
    label: IDWriteTextFormat,
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
        unsafe {
            label
                .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
                .call("SetWordWrapping")?;
        }
        Ok(Self {
            dwrite: gfx.dwrite(),
            text,
            label,
        })
    }

    fn text_format(&self, style: TextStyle) -> &IDWriteTextFormat {
        let i = TextStyle::ALL.iter().position(|&s| s == style).unwrap_or(1);
        &self.text[i]
    }

    fn size_of(&self, text: &str, format: &IDWriteTextFormat, max_width: f32) -> (f32, f32) {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut metrics = DWRITE_TEXT_METRICS::default();
        let measured = unsafe {
            self.dwrite
                .CreateTextLayout(&wide, format, max_width.max(1.0), f32::MAX)
                .and_then(|layout| layout.GetMetrics(&mut metrics))
        };
        match measured {
            Ok(()) => (
                metrics.widthIncludingTrailingWhitespace.ceil(),
                metrics.height.ceil(),
            ),
            Err(_) => (0.0, 0.0),
        }
    }
}

impl Measure for Formats {
    fn text(&self, text: &str, style: TextStyle, max_width: f32) -> (f32, f32) {
        self.size_of(text, self.text_format(style), max_width)
    }

    fn label(&self, label: &str) -> (f32, f32) {
        self.size_of(label, &self.label, f32::MAX / 4.0)
    }
}

/// What the pointer and the keyboard do now.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Interaction {
    pub hover: Option<usize>,
    pub pressed: Option<usize>,
    pub focus: Option<usize>,
    /// Show the focus ring. It shows after the user uses the keyboard, as in Windows.
    pub cues: bool,
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
            Content::Button { label, accent } => {
                let state = State {
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
struct State {
    hover: bool,
    pressed: bool,
    enabled: bool,
}

fn button(
    dc: &ID2D1DeviceContext,
    theme: &Theme,
    r: Rect,
    accent: bool,
    s: State,
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

/// A rounded rectangle with a fill and a 1 DIP border inside its edge.
fn panel(
    dc: &ID2D1DeviceContext,
    r: Rect,
    radius: f32,
    fill: D2D1_COLOR_F,
    border: D2D1_COLOR_F,
) -> windows::core::Result<()> {
    unsafe {
        let shape = rounded(r, 0.0, radius);
        dc.FillRoundedRectangle(&shape, &dc.CreateSolidColorBrush(&fill, None)?);
        let edge = rounded(r, 0.5, radius);
        dc.DrawRoundedRectangle(&edge, &dc.CreateSolidColorBrush(&border, None)?, 1.0, None);
    }
    Ok(())
}

fn focus_ring(dc: &ID2D1DeviceContext, theme: &Theme, r: Rect) -> windows::core::Result<()> {
    unsafe {
        let ring = rounded(r, -FOCUS_GAP, BUTTON_RADIUS + FOCUS_GAP);
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

/// `r` made smaller by `inset` on each side (larger if `inset` is negative), with rounded
/// corners.
fn rounded(r: Rect, inset: f32, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: r.x + inset,
            top: r.y + inset,
            right: r.x + r.w - inset,
            bottom: r.y + r.h - inset,
        },
        radiusX: (radius - inset.max(0.0)).max(0.0),
        radiusY: (radius - inset.max(0.0)).max(0.0),
    }
}
