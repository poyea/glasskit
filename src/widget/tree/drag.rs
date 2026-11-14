// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Drag and drop: where a drag starts, and where a dragged widget goes when it is dropped.

use super::*;

impl Tree {
    /// Whether a widget is `ancestor`, or is in it.
    pub fn within(&self, mut index: usize, ancestor: usize) -> bool {
        loop {
            if index == ancestor {
                return true;
            }
            match self.nodes[index].parent {
                Some(parent) => index = parent,
                None => return false,
            }
        }
    }

    /// The widget that a drag from this point moves: the innermost one that the user can
    /// drag.
    pub fn drag_source_at(&self, x: f32, y: f32) -> Option<usize> {
        let top = self
            .draw_order()
            .into_iter()
            .rev()
            .find(|&i| self.shows_at(i, x, y))?;
        let mut at = Some(top);
        while let Some(index) = at {
            let n = &self.nodes[index];
            if n.draggable && n.enabled {
                return Some(index);
            }
            at = n.parent;
        }
        None
    }

    /// Where the dragged widget `item` goes, if the user drops it at this point.
    pub fn drop_at(&self, x: f32, y: f32, item: usize) -> Option<DropPlace> {
        let target = self.draw_order().into_iter().rev().find(|&i| {
            self.nodes[i].drop_target && self.shows_at(i, x, y) && !self.within(i, item)
        })?;
        let node = &self.nodes[target];
        let c = node.container.as_ref()?;
        let own = &c.children[self.own_start(target).min(c.children.len())..];
        let others: Vec<usize> = own.iter().copied().filter(|&ch| ch != item).collect();
        let shown: Vec<usize> = others
            .iter()
            .copied()
            .filter(|&ch| self.present(ch))
            .collect();
        let along = |r: Rect| match c.axis {
            Axis::Row => (r.x, r.w),
            Axis::Column | Axis::Stack => (r.y, r.h),
        };
        let pointer = if c.axis == Axis::Row { x } else { y };
        let at = match c.axis {
            Axis::Stack => shown.len(),
            Axis::Row | Axis::Column => shown
                .iter()
                .position(|&ch| {
                    let (start, length) = along(self.nodes[ch].rect);
                    pointer < start + length / 2.0
                })
                .unwrap_or(shown.len()),
        };
        let index = shown.get(at).map_or(others.len(), |ch| {
            others.iter().position(|o| o == ch).unwrap_or(others.len())
        });
        let changes = self.nodes[item].parent != Some(target)
            || own.iter().position(|&ch| ch == item) != Some(index);

        let inner = node.rect.inset(c.padding);
        let before = at.checked_sub(1).and_then(|i| shown.get(i));
        let edge = match (before, shown.get(at)) {
            (Some(&before), _) => {
                let (start, length) = along(self.nodes[before].rect);
                start + length + c.gap / 2.0
            }
            (None, Some(&after)) => {
                (along(self.nodes[after].rect).0 - c.gap / 2.0).max(along(inner).0)
            }
            (None, None) => along(inner).0,
        };
        let line = match c.axis {
            Axis::Row => Some(Rect::new(edge - 1.0, inner.y, 2.0, inner.h)),
            Axis::Column => Some(Rect::new(inner.x, edge - 1.0, inner.w, 2.0)),
            Axis::Stack => None,
        };
        Some(DropPlace {
            target,
            index,
            changes,
            line: line.filter(|_| changes),
        })
    }
}
