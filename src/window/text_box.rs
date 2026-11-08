// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The geometry and the drawing of a text box: where its text, caret, and selection are,
//! from a DirectWrite layout. All positions are in DIPs in the window, and text positions
//! are in UTF-16 units.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE, ID2D1DeviceContext,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_HIT_TEST_METRICS, DWRITE_LINE_METRICS, DWRITE_TEXT_METRICS, IDWriteTextLayout,
};
use windows::core::BOOL;
use windows_numerics::Vector2;

use super::draw::{Formats, Interaction, d2d};
use crate::render;
use crate::theme::Theme;
use crate::widget::TextStyle;
use crate::widget::tree::{Content, Node, Rect, TEXT_BOX_PADDING};

const RADIUS: f32 = 4.0;

/// The text of a box, laid out where it shows.
pub(super) struct Field {
    layout: IDWriteTextLayout,
    /// Where the layout starts, after the scroll.
    origin: Vector2,
    /// The area inside the padding.
    inner: Rect,
    multiline: bool,
}

impl Field {
    pub fn new(
        formats: &Formats,
        rect: Rect,
        text: &str,
        multiline: bool,
        scroll: f32,
    ) -> windows::core::Result<Self> {
        let (px, py) = TEXT_BOX_PADDING;
        let inner = Rect::new(
            rect.x + px,
            rect.y + py,
            rect.w - 2.0 * px,
            rect.h - 2.0 * py,
        );
        let format = if multiline {
            formats.text_format(TextStyle::Body)
        } else {
            &formats.line
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            formats
                .dwrite
                .CreateTextLayout(&wide, format, inner.w.max(1.0), inner.h.max(1.0))?
        };
        let origin = if multiline {
            Vector2 {
                X: inner.x,
                Y: inner.y - scroll,
            }
        } else {
            Vector2 {
                X: inner.x - scroll,
                Y: inner.y,
            }
        };
        Ok(Self {
            layout,
            origin,
            inner,
            multiline,
        })
    }

    /// The caret at a text position: its left edge, its top, and its height.
    pub fn caret(&self, position: u32) -> (f32, f32, f32) {
        let (mut x, mut y) = (0.0, 0.0);
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        unsafe {
            let _ = self
                .layout
                .HitTestTextPosition(position, false, &mut x, &mut y, &mut m);
        }
        (self.origin.X + x, self.origin.Y + m.top, m.height)
    }

    /// The text position nearest to a point.
    pub fn hit(&self, x: f32, y: f32) -> u32 {
        let (mut trailing, mut inside) = (BOOL(0), BOOL(0));
        let mut m = DWRITE_HIT_TEST_METRICS::default();
        unsafe {
            let _ = self.layout.HitTestPoint(
                x - self.origin.X,
                y - self.origin.Y,
                &mut trailing,
                &mut inside,
                &mut m,
            );
        }
        m.textPosition + if trailing.as_bool() { m.length } else { 0 }
    }

    /// The position one line up or down, near the same left edge.
    pub fn vertical(&self, position: u32, down: bool) -> u32 {
        let (x, top, height) = self.caret(position);
        let y = if down {
            top + height * 1.5
        } else {
            top - height * 0.5
        };
        self.hit(x, y)
    }

    /// The start or the end of the line that has the position.
    pub fn line_edge(&self, position: u32, end: bool) -> u32 {
        let lines = self.lines();
        let mut start = 0;
        for (i, line) in lines.iter().enumerate() {
            let next = start + line.length;
            let last = i + 1 == lines.len();
            if position < next || last {
                if !end {
                    return start;
                }
                // A wrapped line ends before its trailing spaces, so the caret stays on it.
                let tail = if line.newlineLength > 0 || last {
                    line.newlineLength
                } else {
                    line.trailingWhitespaceLength
                };
                return next - tail;
            }
            start = next;
        }
        0
    }

    /// The scroll that shows the caret at this position, and has no empty space at the end.
    pub fn reveal(&self, position: u32, scroll: f32) -> f32 {
        let (x, top, height) = self.caret(position);
        let (from, to, view) = if self.multiline {
            let top = top - self.origin.Y;
            (top, top + height, self.inner.h)
        } else {
            let x = x - self.origin.X;
            // One DIP more, for the caret at the end.
            (x, x + 1.0, self.inner.w)
        };
        let scroll = if from < scroll {
            from
        } else if to > scroll + view {
            to - view
        } else {
            scroll
        };
        scroll.clamp(0.0, self.max_scroll())
    }

    /// The largest scroll.
    pub fn max_scroll(&self) -> f32 {
        let mut m = DWRITE_TEXT_METRICS::default();
        unsafe {
            let _ = self.layout.GetMetrics(&mut m);
        }
        if self.multiline {
            (m.height - self.inner.h).max(0.0)
        } else {
            (m.widthIncludingTrailingWhitespace + 1.0 - self.inner.w).max(0.0)
        }
    }

    /// The height of one line.
    pub fn line_height(&self) -> f32 {
        self.caret(0).2
    }

    fn lines(&self) -> Vec<DWRITE_LINE_METRICS> {
        let mut lines = vec![DWRITE_LINE_METRICS::default(); 4];
        let mut count = 0;
        loop {
            match unsafe { self.layout.GetLineMetrics(Some(&mut lines), &mut count) } {
                Err(_) if count as usize > lines.len() => {
                    lines.resize(count as usize, DWRITE_LINE_METRICS::default());
                }
                Err(_) => return Vec::new(),
                Ok(()) => break,
            }
        }
        lines.truncate(count as usize);
        lines
    }

    /// The rectangles of a range of text, for the selection.
    fn ranges(&self, start: u32, length: u32) -> Vec<D2D_RECT_F> {
        let mut found = vec![DWRITE_HIT_TEST_METRICS::default(); 4];
        let mut count = 0;
        loop {
            let (x, y) = (self.origin.X, self.origin.Y);
            match unsafe {
                self.layout
                    .HitTestTextRange(start, length, x, y, Some(&mut found), &mut count)
            } {
                Err(_) if count as usize > found.len() => {
                    found.resize(count as usize, DWRITE_HIT_TEST_METRICS::default());
                }
                Err(_) => return Vec::new(),
                Ok(()) => break,
            }
        }
        found
            .iter()
            .take(count as usize)
            .map(|m| render::rect(m.left, m.top, m.width, m.height))
            .collect()
    }
}

/// Draw a text box: its panel, the selection, the text or the placeholder, and the caret.
pub(super) fn draw(
    dc: &ID2D1DeviceContext,
    theme: &Theme,
    formats: &Formats,
    node: &Node,
    index: usize,
    now: Interaction,
) -> windows::core::Result<()> {
    let Content::TextBox {
        edit,
        placeholder,
        scroll,
    } = &node.content
    else {
        return Ok(());
    };
    let r = node.rect;
    let focused = node.enabled && now.focus == Some(index);
    let t = theme;
    let mut fill = if focused {
        t.field
    } else if now.hover == Some(index) {
        t.button_hover
    } else {
        t.button
    };
    if !node.enabled {
        fill.a *= 0.4;
    }
    let outer = d2d(r);
    render::panel(dc, outer, RADIUS, &fill, &t.button_border)?;
    unsafe {
        // The bottom edge: a thin line, or a thick accent line with the focus.
        let (edge, color) = if focused {
            (2.0, t.accent)
        } else {
            (1.0, t.subtle_text)
        };
        let bottom = D2D_RECT_F {
            top: outer.bottom - edge,
            ..outer
        };
        dc.PushAxisAlignedClip(&bottom, D2D1_ANTIALIAS_MODE_ALIASED);
        dc.FillRoundedRectangle(
            &render::rounded(outer, 0.0, RADIUS),
            &dc.CreateSolidColorBrush(&color, None)?,
        );
        dc.PopAxisAlignedClip();
    }

    let field = Field::new(formats, r, edit.text(), edit.multiline(), *scroll)?;
    let (px, _) = TEXT_BOX_PADDING;
    // The text shows up to the border; one DIP more at the sides leaves room for the caret.
    let clip = D2D_RECT_F {
        left: r.x + px - 1.0,
        top: r.y + 1.0,
        right: r.x + r.w - px + 1.0,
        bottom: r.y + r.h - 1.0,
    };
    unsafe {
        dc.PushAxisAlignedClip(&clip, D2D1_ANTIALIAS_MODE_ALIASED);
        let selection = edit.selection();
        if focused && !selection.is_empty() {
            let start = edit.utf16_at(selection.start);
            let length = edit.utf16_at(selection.end) - start;
            let mut color = t.accent;
            color.a *= 0.4;
            let brush = dc.CreateSolidColorBrush(&color, None)?;
            for rect in field.ranges(start, length) {
                dc.FillRectangle(&rect, &brush);
            }
        }
        let shown = if edit.text().is_empty() && !placeholder.is_empty() {
            let hint = Field::new(formats, r, placeholder, edit.multiline(), 0.0)?;
            Some((hint.layout, hint.origin, t.subtle_text))
        } else if !edit.text().is_empty() {
            let color = if node.enabled { t.text } else { t.subtle_text };
            Some((field.layout.clone(), field.origin, color))
        } else {
            None
        };
        if let Some((layout, origin, color)) = shown {
            dc.DrawTextLayout(
                origin,
                &layout,
                &dc.CreateSolidColorBrush(&color, None)?,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
        if focused && now.active && !now.caret_off {
            let (x, top, height) = field.caret(edit.utf16_at(edit.caret()));
            dc.FillRectangle(
                &render::rect(x.round(), top, 1.0, height),
                &dc.CreateSolidColorBrush(&t.text, None)?,
            );
        }
        dc.PopAxisAlignedClip();
    }
    Ok(())
}
