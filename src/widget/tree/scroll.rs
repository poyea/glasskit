// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Scrolling: the clip of a widget, the wheel, and moving a scroll to show a widget.

use super::*;

impl Tree {
    /// Place the children of a scroll as a column at their full height, moved up by the
    /// offset. The offset stays between the top and the end of the children.
    pub(super) fn arrange_scroll(
        &mut self,
        index: usize,
        c: &Container,
        rect: Rect,
        offset: f32,
        m: &dyn Measure,
    ) {
        let (_, extent) = self.measure_container(c, rect.w, f32::MAX / 4.0, m);
        let offset = offset.clamp(0.0, (extent - rect.h).max(0.0));
        self.nodes[index].content = Content::Scroll { offset, extent };
        let full = Rect::new(rect.x, rect.y - offset, rect.w, extent);
        self.arrange_column(c, full.inset(c.padding), m);
    }

    /// The part of the window where a widget can show: the scrolls around it cut it. `None`
    /// means that no scroll is around it.
    pub fn clip(&self, index: usize) -> Option<Rect> {
        let mut clip: Option<Rect> = None;
        let mut at = self.nodes[index].parent;
        while let Some(parent) = at {
            if let Content::Scroll { .. } = self.nodes[parent].content {
                let r = self.nodes[parent].rect;
                clip = Some(clip.map_or(r, |c| c.intersect(r)));
            }
            at = self.nodes[parent].parent;
        }
        clip
    }

    /// Whether the point is in the part of the widget that shows.
    pub(super) fn shows_at(&self, index: usize, x: f32, y: f32) -> bool {
        self.nodes[index].rect.contains(x, y) && self.clip(index).is_none_or(|c| c.contains(x, y))
    }

    /// The innermost scroll at the point that can move by `delta` DIPs. A positive delta
    /// moves the children up, to show more of the end.
    pub fn scroll_at(&self, x: f32, y: f32, delta: f32) -> Option<usize> {
        self.draw_order().into_iter().rev().find(|&i| {
            self.shows_at(i, x, y)
                && match self.nodes[i].content {
                    Content::Scroll { offset, extent } => {
                        let max = (extent - self.nodes[i].rect.h).max(0.0);
                        (delta > 0.0 && offset < max) || (delta < 0.0 && offset > 0.0)
                    }
                    _ => false,
                }
        })
    }

    /// Move a scroll by `delta` DIPs. The next layout keeps it in its range.
    pub fn scroll_by(&mut self, index: usize, delta: f32) {
        if let Some(Content::Scroll { offset, extent }) =
            self.nodes.get_mut(index).map(|n| &mut n.content)
        {
            *offset = (*offset + delta).clamp(0.0, *extent);
        }
    }

    /// Scroll the innermost scroll around a widget, so that all of the widget shows, or its
    /// top if it is higher than the scroll. It gives true if the scroll moved.
    pub fn reveal(&mut self, index: usize) -> bool {
        let mut at = self.nodes.get(index).and_then(|n| n.parent);
        while let Some(parent) = at {
            if let Content::Scroll { offset, extent } = self.nodes[parent].content {
                let view = self.nodes[parent].rect;
                let r = self.nodes[index].rect;
                let mut moved = offset;
                if r.y + r.h > view.y + view.h {
                    moved += r.y + r.h - (view.y + view.h);
                }
                if r.y - (moved - offset) < view.y {
                    moved = offset - (view.y - r.y);
                }
                let moved = moved.clamp(0.0, (extent - view.h).max(0.0));
                self.nodes[parent].content = Content::Scroll {
                    offset: moved,
                    extent,
                };
                return moved != offset;
            }
            at = self.nodes[parent].parent;
        }
        false
    }
}
