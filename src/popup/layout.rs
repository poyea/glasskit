// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The size and the parts of a popup, in pixels at the display scale.

use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, IDWriteTextFormat, IDWriteTextLayout,
};

use super::glass::{self, Graphics};
use super::markup;
use super::{PopupStyle, Prompt, Response};
use crate::error::Result;

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

pub(super) struct Fonts {
    pub(super) prompt: IDWriteTextFormat,
    pub(super) button: IDWriteTextFormat,
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
    /// The choices in order, then the dismiss button.
    pub(super) buttons: Vec<Button>,
}

/// What a popup shows, ready for layout.
pub(super) struct Content {
    pub(super) text: String,
    pub(super) spans: Vec<markup::Span>,
    pub(super) choices: Vec<String>,
    pub(super) dismiss: String,
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
    let mut y = pad + glass::layout_height(&prompt_layout)?.ceil() + s(TEXT_GAP);

    let mut buttons = Vec::with_capacity(choices.len() + 1);
    for (i, choice) in choices.iter().enumerate() {
        let choice = choice.as_str();
        let text_w = inner - 2.0 * s(BUTTON_TEXT_PADDING);
        let label = gfx.text_layout(choice, &fonts.button, text_w)?;
        let h = (glass::layout_height(&label)?.ceil() + 2.0 * s(BUTTON_TEXT_PADDING))
            .max(s(BUTTON_MIN_HEIGHT));
        buttons.push(Button {
            rect: glass::rect(pad, y, inner, h),
            label: choice.encode_utf16().collect(),
            response: Response::Choice(i),
        });
        y += h + s(BUTTON_GAP);
    }
    let dismiss_text = gfx.text_layout(dismiss, &fonts.button, inner)?;
    let dismiss_w = (glass::layout_width(&dismiss_text)?.ceil() + 2.0 * s(DISMISS_TEXT_PADDING))
        .max(s(DISMISS_MIN_WIDTH));
    buttons.push(Button {
        rect: glass::rect(width - pad - dismiss_w, y, dismiss_w, s(DISMISS_HEIGHT)),
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
        buttons,
    })
}
