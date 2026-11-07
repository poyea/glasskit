// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The size and the parts of a popup, in pixels at the display scale.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};

use super::markup;
use super::{PopupStyle, Prompt, Response};
use crate::error::{Call, Result};
use crate::render::{self, Graphics};

const PADDING: f32 = 18.0;
const TEXT_GAP: f32 = 14.0;
const BUTTON_MIN_HEIGHT: f32 = 36.0;
const BUTTON_TEXT_PADDING: f32 = 9.0;
const BUTTON_GAP: f32 = 8.0;
const BUTTON_RADIUS: f32 = 6.0;
const WINDOW_RADIUS: f32 = 8.0;
const DISMISS_MIN_WIDTH: f32 = 72.0;
const DISMISS_TEXT_PADDING: f32 = 12.0;
const DISMISS_HEIGHT: f32 = 28.0;
const IMAGE_MAX_HEIGHT: f32 = 240.0;
const INPUT_HEIGHT: f32 = 34.0;
const INPUT_TEXT_PADDING: f32 = 10.0;

/// The text formats, and the factory for text that changes while the popup shows.
pub(super) struct Fonts {
    pub(super) prompt: IDWriteTextFormat,
    pub(super) button: IDWriteTextFormat,
    pub(super) input: IDWriteTextFormat,
    pub(super) dwrite: IDWriteFactory,
}

impl Fonts {
    pub(super) fn new(gfx: &Graphics, scale: f32, style: &PopupStyle) -> Result<Self> {
        Ok(Self {
            prompt: gfx.text_format(
                style.text_size * scale,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                false,
            )?,
            button: gfx.text_format(
                style.button_text_size * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                true,
            )?,
            input: input_format(gfx, style.button_text_size * scale)?,
            dwrite: gfx.dwrite(),
        })
    }
}

pub(super) struct Button {
    pub(super) rect: D2D_RECT_F,
    pub(super) label: Vec<u16>,
    pub(super) response: Response,
}

pub(super) struct Layout {
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) pad: f32,
    pub(super) radius: f32,
    pub(super) window_radius: f32,
    pub(super) prompt: IDWriteTextLayout,
    /// Where the image of the prompt shows, if it has one.
    pub(super) image: Option<D2D_RECT_F>,
    /// Where the input box shows, if the prompt has one.
    pub(super) input: Option<D2D_RECT_F>,
    /// The space between the edge of the input box and its text.
    pub(super) input_padding: f32,
    /// The choices in order, then the dismiss button.
    pub(super) buttons: Vec<Button>,
}

/// What a popup shows, ready for layout.
pub(super) struct Content {
    pub(super) text: String,
    pub(super) spans: Vec<markup::Span>,
    pub(super) choices: Vec<String>,
    pub(super) dismiss: String,
    pub(super) image: Option<(u32, u32)>,
    pub(super) input: bool,
}

impl Content {
    pub(super) fn of(prompt: &Prompt, dismiss: String) -> Self {
        let (text, spans) = if prompt.markup {
            markup::parse(&prompt.text)
        } else {
            (prompt.text.clone(), Vec::new())
        };
        Self {
            text,
            spans,
            choices: prompt.choices.clone(),
            dismiss,
            image: prompt.image.as_ref().map(|i| (i.width, i.height)),
            input: prompt.input.is_some(),
        }
    }
}

pub(super) fn build(
    scale: f32,
    width: f32,
    gfx: &Graphics,
    fonts: &Fonts,
    content: &Content,
) -> Result<Layout> {
    let (prompt, spans, choices, dismiss) = (
        content.text.as_str(),
        content.spans.as_slice(),
        content.choices.as_slice(),
        content.dismiss.as_str(),
    );
    let s = |v: f32| (v * scale).round();
    let width = s(width);
    let pad = s(PADDING);
    let inner = width - 2.0 * pad;

    let prompt_layout = gfx.text_layout(prompt, &fonts.prompt, inner)?;
    markup::apply(&prompt_layout, spans)?;
    let mut y = pad + render::layout_height(&prompt_layout)?.ceil() + s(TEXT_GAP);
    let image = content.image.map(|size| {
        let (w, h) = fit(size, scale, inner, s(IMAGE_MAX_HEIGHT));
        let rect = render::rect(pad + ((inner - w) / 2.0).round(), y, w, h);
        y += h + s(TEXT_GAP);
        rect
    });
    let input = content.input.then(|| {
        let rect = render::rect(pad, y, inner, s(INPUT_HEIGHT));
        y += s(INPUT_HEIGHT) + s(TEXT_GAP);
        rect
    });

    let mut buttons = Vec::with_capacity(choices.len() + 1);
    for (i, choice) in choices.iter().enumerate() {
        let choice = choice.as_str();
        let text_w = inner - 2.0 * s(BUTTON_TEXT_PADDING);
        let label = gfx.text_layout(choice, &fonts.button, text_w)?;
        let h = (render::layout_height(&label)?.ceil() + 2.0 * s(BUTTON_TEXT_PADDING))
            .max(s(BUTTON_MIN_HEIGHT));
        buttons.push(Button {
            rect: render::rect(pad, y, inner, h),
            label: choice.encode_utf16().collect(),
            response: Response::Choice(i),
        });
        y += h + s(BUTTON_GAP);
    }
    let dismiss_text = gfx.text_layout(dismiss, &fonts.button, inner)?;
    let dismiss_w = (render::layout_width(&dismiss_text)?.ceil() + 2.0 * s(DISMISS_TEXT_PADDING))
        .max(s(DISMISS_MIN_WIDTH));
    buttons.push(Button {
        rect: render::rect(width - pad - dismiss_w, y, dismiss_w, s(DISMISS_HEIGHT)),
        label: dismiss.encode_utf16().collect(),
        response: Response::Dismissed,
    });
    y += s(DISMISS_HEIGHT) + pad;

    Ok(Layout {
        width,
        height: y,
        pad,
        radius: s(BUTTON_RADIUS),
        window_radius: s(WINDOW_RADIUS),
        prompt: prompt_layout,
        image,
        input,
        input_padding: s(INPUT_TEXT_PADDING),
        buttons,
    })
}

/// One line of text, at the left and in the middle of its height.
fn input_format(gfx: &Graphics, size: f32) -> Result<IDWriteTextFormat> {
    let format = gfx.text_format(size, DWRITE_FONT_WEIGHT_NORMAL, false)?;
    unsafe {
        format
            .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
            .call("SetParagraphAlignment")?;
        format
            .SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)
            .call("SetWordWrapping")?;
    }
    Ok(format)
}

/// The size of an image of `(width, height)` pixels at this scale, smaller if needed to fit
/// in `max_width` and `max_height`, with the same aspect ratio. It is whole pixels.
fn fit((width, height): (u32, u32), scale: f32, max_width: f32, max_height: f32) -> (f32, f32) {
    let (w, h) = (width as f32 * scale, height as f32 * scale);
    let shrink = (max_width / w).min(max_height / h).min(1.0);
    ((w * shrink).round().max(1.0), (h * shrink).round().max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_image_keeps_its_size_at_the_scale() {
        assert_eq!(fit((100, 50), 1.5, 324.0, 360.0), (150.0, 75.0));
    }

    #[test]
    fn a_large_image_shrinks_and_keeps_its_shape() {
        assert_eq!(fit((1000, 500), 1.0, 324.0, 240.0), (324.0, 162.0));
        assert_eq!(fit((300, 600), 1.0, 324.0, 240.0), (120.0, 240.0));
    }
}
