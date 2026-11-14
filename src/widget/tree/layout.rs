// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Sizes and places of the widgets: measure them, then arrange them in rows, columns and stacks.

use super::*;

impl Tree {
    /// Place all widgets in a window of this size.
    pub fn layout(&mut self, width: f32, height: f32, m: &dyn Measure) {
        if let Some(root) = self.root {
            self.arrange(root, Rect::new(0.0, 0.0, width, height), m);
        }
    }

    /// The size that a widget wants, if it can be at most `max_w` wide.
    pub fn measure(&self, index: usize, max_w: f32, max_h: f32, m: &dyn Measure) -> (f32, f32) {
        let n = &self.nodes[index];
        let max_w = match n.width {
            Length::Fixed(w) => w,
            _ => max_w,
        };
        let (w, h) = match &n.container {
            Some(c) => self.measure_container(c, max_w, max_h, m),
            None => intrinsic(&n.content, self.compact(index), max_w, m),
        };
        let pick = |length: Length, content: f32| match length {
            Length::Fixed(v) => v,
            Length::Auto | Length::Fill(_) => content,
        };
        (pick(n.width, w), pick(n.height, h))
    }

    pub(super) fn measure_container(
        &self,
        c: &Container,
        max_w: f32,
        max_h: f32,
        m: &dyn Measure,
    ) -> (f32, f32) {
        let p = c.padding;
        let inner_w = (max_w - p.left - p.right).max(0.0);
        let inner_h = (max_h - p.top - p.bottom).max(0.0);
        let shown = c.children.iter().filter(|&&child| self.present(child));
        let gaps = c.gap * shown.clone().count().saturating_sub(1) as f32;
        let sizes = shown.map(|&child| self.measure(child, inner_w, inner_h, m));
        let (w, h) = match c.axis {
            Axis::Row => sizes.fold((gaps, 0.0f32), |(w, h), (cw, ch)| (w + cw, h.max(ch))),
            Axis::Column => sizes.fold((0.0f32, gaps), |(w, h), (cw, ch)| (w.max(cw), h + ch)),
            Axis::Stack => sizes.fold((0.0f32, 0.0f32), |(w, h), (cw, ch)| (w.max(cw), h.max(ch))),
        };
        (w + p.left + p.right, h + p.top + p.bottom)
    }

    fn arrange(&mut self, index: usize, rect: Rect, m: &dyn Measure) {
        self.nodes[index].rect = rect;
        let Some(mut c) = self.nodes[index].container.clone() else {
            return;
        };
        c.children.retain(|&child| self.present(child));
        if let Content::Scroll { offset, .. } = self.nodes[index].content {
            self.arrange_scroll(index, &c, rect, offset, m);
            return;
        }
        let inner = rect.inset(c.padding);
        match c.axis {
            Axis::Row => self.arrange_row(&c, inner, m),
            Axis::Column => self.arrange_column(&c, inner, m),
            Axis::Stack => {
                for &child in &c.children {
                    let (w, h) = self.cross_size(child, inner.w, inner.h, c.align, m);
                    let x = offset(inner.x, inner.w, w, c.align);
                    let y = offset(inner.y, inner.h, h, c.align);
                    self.arrange(child, Rect::new(x, y, w, h), m);
                }
            }
        }
    }

    fn arrange_row(&mut self, c: &Container, inner: Rect, m: &dyn Measure) {
        let widths = self.main_sizes(c, inner.w, |tree, child| {
            let n = &tree.nodes[child];
            match n.width {
                Length::Fill(weight) => Err(weight),
                _ => Ok(tree.measure(child, inner.w, inner.h, m).0),
            }
        });
        let mut x = inner.x;
        for (&child, w) in c.children.iter().zip(widths) {
            let h = match self.nodes[child].height {
                Length::Fixed(h) => h,
                Length::Fill(_) => inner.h,
                Length::Auto if c.align == Align::Stretch => inner.h,
                Length::Auto => self.measure(child, w, inner.h, m).1.min(inner.h),
            };
            let y = offset(inner.y, inner.h, h, c.align);
            self.arrange(child, Rect::new(x, y, w, h), m);
            x += w + c.gap;
        }
    }

    pub(super) fn arrange_column(&mut self, c: &Container, inner: Rect, m: &dyn Measure) {
        // The width comes first, because a text that wraps is higher when it is narrower.
        let widths: Vec<f32> = c
            .children
            .iter()
            .map(|&child| match self.nodes[child].width {
                Length::Fixed(w) => w,
                Length::Fill(_) => inner.w,
                Length::Auto if c.align == Align::Stretch => inner.w,
                Length::Auto => self.measure(child, inner.w, inner.h, m).0.min(inner.w),
            })
            .collect();
        let heights = self.main_sizes(c, inner.h, |tree, child| {
            let i = c.children.iter().position(|&x| x == child).unwrap_or(0);
            match tree.nodes[child].height {
                Length::Fill(weight) => Err(weight),
                _ => Ok(tree.measure(child, widths[i], inner.h, m).1),
            }
        });
        let mut y = inner.y;
        for ((&child, w), h) in c.children.iter().zip(widths).zip(heights) {
            let x = offset(inner.x, inner.w, w, c.align);
            self.arrange(child, Rect::new(x, y, w, h), m);
            y += h + c.gap;
        }
    }

    /// The sizes along a row or a column. `size` gives a fixed size, or the weight of a
    /// fill. Fills share the space that the others leave, by weight.
    fn main_sizes(
        &self,
        c: &Container,
        space: f32,
        size: impl Fn(&Tree, usize) -> Result<f32, f32>,
    ) -> Vec<f32> {
        let sizes: Vec<Result<f32, f32>> =
            c.children.iter().map(|&child| size(self, child)).collect();
        let gaps = c.gap * c.children.len().saturating_sub(1) as f32;
        let fixed: f32 = sizes.iter().filter_map(|s| s.ok()).sum();
        let weights: f32 = sizes
            .iter()
            .filter_map(|s| s.err())
            .map(|w| w.max(0.0))
            .sum();
        let free = (space - fixed - gaps).max(0.0);
        sizes
            .into_iter()
            .map(|s| match s {
                Ok(v) => v,
                Err(_) if weights <= 0.0 => 0.0,
                Err(w) => free * w.max(0.0) / weights,
            })
            .collect()
    }

    /// The size of a child in a stack.
    fn cross_size(
        &self,
        child: usize,
        w: f32,
        h: f32,
        align: Align,
        m: &dyn Measure,
    ) -> (f32, f32) {
        let n = &self.nodes[child];
        let (mw, mh) = self.measure(child, w, h, m);
        let pick = |length: Length, space: f32, measured: f32| match length {
            Length::Fixed(v) => v,
            Length::Fill(_) => space,
            Length::Auto if align == Align::Stretch => space,
            Length::Auto => measured.min(space),
        };
        (pick(n.width, w, mw), pick(n.height, h, mh))
    }
}

fn intrinsic(content: &Content, compact: bool, max_w: f32, m: &dyn Measure) -> (f32, f32) {
    match content {
        Content::Blank
        | Content::Card { .. }
        | Content::Scroll { .. }
        | Content::SideBar { .. } => (0.0, 0.0),
        Content::SideToggle => (SIDE_COMPACT_WIDTH, NAV_ITEM_HEIGHT),
        Content::NavItem { .. } if compact => (SIDE_COMPACT_WIDTH, NAV_ITEM_HEIGHT),
        Content::Text { text, style } => m.text(text, *style, max_w),
        Content::Button { icon: true, .. } => (BUTTON_MIN.1, BUTTON_MIN.1),
        Content::Button { label, .. } => {
            let (w, h) = m.label(label);
            (
                (w + 2.0 * BUTTON_PADDING.0).max(BUTTON_MIN.0),
                (h + 2.0 * BUTTON_PADDING.1).max(BUTTON_MIN.1),
            )
        }
        Content::NavItem { label, icon, .. } => {
            let (w, h) = m.label(label);
            let icon = icon.as_ref().map_or(0.0, |_| NAV_ICON.0 + NAV_ICON.1);
            (
                w + icon + NAV_ITEM_PADDING.0 + NAV_ITEM_PADDING.1,
                h.max(NAV_ITEM_HEIGHT),
            )
        }
        Content::TextBox { edit, .. } => {
            let (px, py) = TEXT_BOX_PADDING;
            // A box with many lines grows with its text. An empty box has one line.
            let (sample, wrap) = match edit.text() {
                text if edit.multiline() && !text.is_empty() => (text, max_w - 2.0 * px),
                _ => (" ", f32::MAX / 4.0),
            };
            let (_, h) = m.text(sample, TextStyle::Body, wrap.max(1.0));
            (
                TEXT_BOX_WIDTH.min(max_w),
                (h + 2.0 * py).max(TEXT_BOX_MIN_HEIGHT),
            )
        }
    }
}

fn offset(start: f32, space: f32, size: f32, align: Align) -> f32 {
    match align {
        Align::Start | Align::Stretch => start,
        Align::Center => start + ((space - size) / 2.0).max(0.0),
        Align::End => start + (space - size).max(0.0),
    }
}
