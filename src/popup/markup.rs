// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A small markup for the prompt text: `**bold**`, `*italic*`, and `` `code` ``.

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_RANGE, IDWriteTextLayout,
};
use windows::core::w;

use crate::error::{Call, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    Bold,
    Italic,
    Code,
}

impl Style {
    const ALL: [Style; 3] = [Style::Bold, Style::Italic, Style::Code];

    fn marker(self) -> &'static str {
        match self {
            Style::Bold => "**",
            Style::Italic => "*",
            Style::Code => "`",
        }
    }
}

/// A part of the plain text with a style. `start` and `len` count UTF-16 units, as
/// DirectWrite does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) start: u32,
    pub(super) len: u32,
    pub(super) style: Style,
}

/// The plain text and its styled parts. A backslash makes the next character plain. A
/// marker without its end stays in the text as it is.
pub(super) fn parse(source: &str) -> (String, Vec<Span>) {
    let mut text = String::with_capacity(source.len());
    let mut units: u32 = 0;
    let mut spans = Vec::new();
    // For each style that is open: where it started, in UTF-16 units and in bytes.
    let mut open: [Option<(u32, usize)>; 3] = [None; 3];
    let mut chars = source.chars().peekable();

    let push = |text: &mut String, units: &mut u32, c: char| {
        text.push(c);
        *units += c.len_utf16() as u32;
    };
    while let Some(c) = chars.next() {
        let in_code = open[Style::Code as usize].is_some();
        let style = match c {
            '\\' if !in_code => {
                if let Some(next) = chars.next() {
                    push(&mut text, &mut units, next);
                } else {
                    push(&mut text, &mut units, c);
                }
                continue;
            }
            '`' => Style::Code,
            '*' if !in_code && chars.peek() == Some(&'*') => {
                chars.next();
                Style::Bold
            }
            '*' if !in_code => Style::Italic,
            _ => {
                push(&mut text, &mut units, c);
                continue;
            }
        };
        match open[style as usize].take() {
            Some((start, _)) if units > start => spans.push(Span {
                start,
                len: units - start,
                style,
            }),
            Some(_) => {}
            None => open[style as usize] = Some((units, text.len())),
        }
    }

    // Put back the markers that did not end, from the last to the first, so that each
    // insert does not move the ones before it.
    let mut unclosed: Vec<(Style, u32, usize)> = Style::ALL
        .into_iter()
        .filter_map(|style| open[style as usize].map(|(at, byte)| (style, at, byte)))
        .collect();
    unclosed.sort_by_key(|&(_, at, _)| std::cmp::Reverse(at));
    for (style, at, byte) in unclosed {
        let marker = style.marker();
        let added = marker.len() as u32;
        text.insert_str(byte, marker);
        for span in &mut spans {
            if span.start >= at {
                span.start += added;
            } else if span.start + span.len > at {
                span.len += added;
            }
        }
    }
    spans.sort_by_key(|span| span.start);
    (text, spans)
}

/// Give the styled parts their look in a text layout of the plain text.
pub(super) fn apply(layout: &IDWriteTextLayout, spans: &[Span]) -> Result<()> {
    for span in spans {
        let range = DWRITE_TEXT_RANGE {
            startPosition: span.start,
            length: span.len,
        };
        unsafe {
            match span.style {
                Style::Bold => layout
                    .SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)
                    .call("SetFontWeight")?,
                Style::Italic => layout
                    .SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)
                    .call("SetFontStyle")?,
                Style::Code => layout
                    .SetFontFamilyName(w!("Consolas"), range)
                    .call("SetFontFamilyName")?,
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, len: u32, style: Style) -> Span {
        Span { start, len, style }
    }

    #[test]
    fn plain_text_stays() {
        assert_eq!(parse("Hello, world"), ("Hello, world".into(), vec![]));
    }

    #[test]
    fn markers_become_styles() {
        let (text, spans) = parse("a **b** *c* `d`");
        assert_eq!(text, "a b c d");
        assert_eq!(
            spans,
            [
                span(2, 1, Style::Bold),
                span(4, 1, Style::Italic),
                span(6, 1, Style::Code)
            ]
        );
    }

    #[test]
    fn styles_can_nest() {
        let (text, spans) = parse("**bold *both***");
        assert_eq!(text, "bold both");
        assert!(spans.contains(&span(0, 9, Style::Bold)));
        assert!(spans.contains(&span(5, 4, Style::Italic)));
    }

    #[test]
    fn code_keeps_stars_and_backslashes() {
        let (text, spans) = parse(r"`a*b\n`");
        assert_eq!(text, r"a*b\n");
        assert_eq!(spans, [span(0, 5, Style::Code)]);
    }

    #[test]
    fn backslash_makes_a_marker_plain() {
        assert_eq!(parse(r"2 \* 3 = 6"), ("2 * 3 = 6".into(), vec![]));
    }

    #[test]
    fn an_open_marker_stays_in_the_text() {
        assert_eq!(parse("5 * 3"), ("5 * 3".into(), vec![]));
        let (text, spans) = parse("**a** and *b");
        assert_eq!(text, "a and *b");
        assert_eq!(spans, [span(0, 1, Style::Bold)]);
    }

    #[test]
    fn an_open_marker_inside_a_span_widens_it() {
        let (text, spans) = parse("**x * y**");
        assert_eq!(text, "x * y");
        assert_eq!(spans, [span(0, 5, Style::Bold)]);
    }

    #[test]
    fn positions_count_utf16_units() {
        let (text, spans) = parse("😀 **憶**");
        assert_eq!(text, "😀 憶");
        assert_eq!(spans, [span(3, 1, Style::Bold)]);
    }

    #[test]
    fn empty_markers_add_no_span() {
        assert_eq!(parse("a****b"), ("ab".into(), vec![]));
    }
}
