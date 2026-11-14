// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text formats of a window, the text sizes for the layout, and the drawing of each
//! widget. All sizes are in DIPs.

use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_WORD_WRAPPING_NO_WRAP,
    IDWriteFactory, IDWriteTextFormat,
};
use windows_numerics::{Matrix3x2, Vector2};

use super::text_box;
use crate::error::{Call, Result};
use crate::render::{self, Graphics};
use crate::theme::Theme;
use crate::theme::rgba;
use crate::widget::TextStyle;
use crate::widget::tree::{
    Content, Measure, NAV_ICON, NAV_ITEM_PADDING, Rect, SIDE_COMPACT_WIDTH, Tree,
};

const BUTTON_RADIUS: f32 = 4.0;
const CARD_RADIUS: f32 = 8.0;
/// The bar that shows where a scroll is: its width, its space from the right edge, and its
/// shortest length, in DIPs.
const SCROLL_BAR: (f32, f32, f32) = (3.0, 3.0, 24.0);
/// The mark of a selected navigation item: a bar at its left edge, in DIPs.
const NAV_MARK: (f32, f32) = (3.0, 16.0);
const FOCUS_GAP: f32 = 3.0;
const FOCUS_WIDTH: f32 = 2.0;
const ICON_SIZE: f32 = 16.0;
/// The pin of a pinned card, in the icon font, and the size of its corner, in DIPs.
const PIN_ICON: &str = "\u{E718}";
const PIN_CORNER: f32 = 30.0;
const DROP_OUTLINE_WIDTH: f32 = 1.5;
/// The three lines of the button of a side bar, in the icon font.
const SIDE_TOGGLE_ICON: &str = "\u{E700}";

pub(super) struct Formats {
    pub(super) dwrite: IDWriteFactory,
    /// One for each [`TextStyle`], in the order of [`TextStyle::ALL`].
    text: Vec<IDWriteTextFormat>,
    label: IDWriteTextFormat,
    /// Body text on one line, for a text box.
    pub(super) line: IDWriteTextFormat,
    icon: IDWriteTextFormat,
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
            icon: gfx.icon_format(ICON_SIZE)?,
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
    pub drag: Option<Dragging>,
}

/// A widget that the pointer drags now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Dragging {
    pub item: usize,
    /// How far the pointer moved the widget, in DIPs.
    pub offset: (f32, f32),
    /// The container that takes the widget, and a line at the place in it.
    pub target: Option<usize>,
    pub line: Option<Rect>,
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
        if now.drag.is_some_and(|d| tree.within(index, d.item)) {
            continue;
        }
        let clip = tree.clip(index);
        if clip.is_some_and(|c| c.intersect(tree.nodes[index].rect).is_empty()) {
            continue;
        }
        if let Some(c) = clip {
            unsafe { dc.PushAxisAlignedClip(&d2d(c), D2D1_ANTIALIAS_MODE_ALIASED) };
        }
        let drawn = widget(dc, tree, index, theme, formats, now);
        if clip.is_some() {
            unsafe { dc.PopAxisAlignedClip() };
        }
        drawn?;
    }
    for index in tree.draw_order() {
        if let Content::Scroll { offset, extent } = tree.nodes[index].content {
            scroll_bar(dc, theme, tree.nodes[index].rect, offset, extent)?;
        }
    }
    if let Some(drag) = now.drag {
        dragged(dc, tree, theme, formats, now, drag)?;
    }
    Ok(())
}

/// The place where a drop goes, and the dragged widget at the pointer, above all others.
fn dragged(
    dc: &ID2D1DeviceContext,
    tree: &Tree,
    theme: &Theme,
    formats: &Formats,
    now: Interaction,
    drag: Dragging,
) -> windows::core::Result<()> {
    if let Some(target) = drag.target {
        let r = tree.nodes[target].rect;
        let clip = tree.clip(target).map_or(r, |c| c.intersect(r));
        unsafe {
            dc.PushAxisAlignedClip(&d2d(clip), D2D1_ANTIALIAS_MODE_ALIASED);
            let half = DROP_OUTLINE_WIDTH / 2.0;
            dc.DrawRoundedRectangle(
                &render::rounded(d2d(r), half, CARD_RADIUS),
                &dc.CreateSolidColorBrush(&theme.accent, None)?,
                DROP_OUTLINE_WIDTH,
                None,
            );
        }
        let line = drag.line.map_or(Ok(()), |line| {
            panel(dc, line, 1.0, theme.accent, theme.accent)
        });
        unsafe { dc.PopAxisAlignedClip() };
        line?;
    }
    let (dx, dy) = drag.offset;
    let mut base = Matrix3x2::default();
    unsafe { dc.GetTransform(&mut base) };
    let moved = Matrix3x2 {
        M31: dx * base.M11 + dy * base.M21 + base.M31,
        M32: dx * base.M12 + dy * base.M22 + base.M32,
        ..base
    };
    unsafe { dc.SetTransform(&moved) };
    let drawn = tree
        .draw_order()
        .into_iter()
        .filter(|&index| tree.within(index, drag.item))
        .try_for_each(|index| widget(dc, tree, index, theme, formats, now));
    unsafe { dc.SetTransform(&base) };
    drawn
}

/// The bar of a scroll whose children are higher than it.
fn scroll_bar(
    dc: &ID2D1DeviceContext,
    theme: &Theme,
    r: Rect,
    offset: f32,
    extent: f32,
) -> windows::core::Result<()> {
    if extent <= r.h || r.h <= 0.0 {
        return Ok(());
    }
    let (width, inset, min) = SCROLL_BAR;
    let length = (r.h * r.h / extent).max(min).min(r.h);
    let y = r.y + (r.h - length) * (offset / (extent - r.h));
    let bar = Rect::new(r.x + r.w - width - inset, y, width, length);
    let mut color = theme.subtle_text;
    color.a *= 0.6;
    panel(dc, bar, width / 2.0, color, color)
}

fn widget(
    dc: &ID2D1DeviceContext,
    tree: &Tree,
    index: usize,
    theme: &Theme,
    formats: &Formats,
    now: Interaction,
) -> windows::core::Result<()> {
    let node = &tree.nodes[index];
    let r = node.rect;
    match &node.content {
        Content::Blank | Content::Scroll { .. } | Content::SideBar { .. } => {}
        Content::SideToggle => {
            let hover = now.hover == Some(index);
            let fill = match (hover, now.pressed == Some(index)) {
                (true, true) => Some(theme.button_pressed),
                (true, false) => Some(theme.button),
                (false, _) => None,
            };
            if let Some(fill) = fill {
                panel(dc, r, BUTTON_RADIUS, fill, rgba(0.0, 0.0, 0.0, 0.0))?;
            }
            draw_text(dc, formats, SIDE_TOGGLE_ICON, &formats.icon, r, theme.text)?;
            if now.cues && now.focus == Some(index) {
                focus_ring(dc, theme, r)?;
            }
        }
        Content::Card { pinned, tint, .. } => {
            let hover = node.clickable && now.hover == Some(index);
            let pressed = hover && now.pressed == Some(index);
            let (fill, border) = match tint.filter(|_| !theme.high_contrast) {
                Some(tint) => {
                    let (red, green, blue) = tint.rgb();
                    let base = if theme.dark { 0.28 } else { 0.2 };
                    let more = match (hover, pressed) {
                        (_, true) => 0.16,
                        (true, false) => 0.08,
                        (false, false) => 0.0,
                    };
                    (
                        rgba(red, green, blue, base + more),
                        rgba(red, green, blue, 0.6),
                    )
                }
                None => {
                    let fill = match (hover, pressed) {
                        (_, true) => theme.button_pressed,
                        (true, false) => theme.button_hover,
                        (false, false) => theme.card,
                    };
                    (fill, theme.card_border)
                }
            };
            panel(dc, r, CARD_RADIUS, fill, border)?;
            if *pinned {
                let corner = Rect::new(r.x + r.w - PIN_CORNER, r.y, PIN_CORNER, PIN_CORNER);
                draw_text(
                    dc,
                    formats,
                    PIN_ICON,
                    &formats.icon,
                    corner,
                    theme.subtle_text,
                )?;
            }
            if node.clickable && now.cues && now.focus == Some(index) {
                focus_ring(dc, theme, r)?;
            }
        }
        Content::Text { text, style } => {
            let color = if node.enabled {
                theme.text
            } else {
                theme.subtle_text
            };
            draw_text(dc, formats, text, formats.text_format(*style), r, color)?;
        }
        Content::TextBox { .. } => text_box::draw(dc, theme, formats, node, index, now)?,
        Content::NavItem {
            label,
            icon,
            selected,
        } => {
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
            let compact = tree.compact(index);
            let (left, right) = NAV_ITEM_PADDING;
            let mut left = left;
            if let Some(icon) = icon {
                let slot = Rect::new(r.x, r.y, SIDE_COMPACT_WIDTH.min(r.w), r.h);
                draw_text(dc, formats, icon, &formats.icon, slot, color)?;
                left += NAV_ICON.0 + NAV_ICON.1;
            }
            if !compact {
                let (_, text_h) = formats.label(label);
                let text = Rect::new(
                    r.x + left,
                    r.y + (r.h - text_h) / 2.0,
                    r.w - left - right,
                    text_h,
                );
                draw_text(dc, formats, label, &formats.line, text, color)?;
            } else if icon.is_none() {
                let first: String = label.chars().take(1).collect();
                draw_text(dc, formats, &first, &formats.label, r, color)?;
            }
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
