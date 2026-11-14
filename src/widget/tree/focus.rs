// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The order of drawing, focus with Tab, and the widget under a click.

use super::*;

impl Tree {
    /// The widgets in drawing order: a parent before its children.
    pub fn draw_order(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.nodes.len());
        if let Some(root) = self.root
            && self.nodes[root].visible
        {
            self.walk(root, &mut out);
        }
        out
    }

    fn walk(&self, index: usize, out: &mut Vec<usize>) {
        out.push(index);
        for &child in self.nodes[index].children() {
            if self.present(child) {
                self.walk(child, out);
            }
        }
    }

    /// The widgets that Tab moves to, in order.
    pub fn focus_order(&self) -> Vec<usize> {
        self.draw_order()
            .into_iter()
            .filter(|&i| self.focusable(i))
            .collect()
    }

    pub fn focusable(&self, index: usize) -> bool {
        self.nodes.get(index).is_some_and(|n| {
            n.enabled && (n.content.interactive() || n.clickable) && self.shown(index)
        })
    }

    /// Whether a widget is in the tree under the root, and it and all widgets around it are
    /// visible.
    pub fn shown(&self, mut index: usize) -> bool {
        loop {
            if !self.present(index) {
                return false;
            }
            if Some(index) == self.root {
                return true;
            }
            match self.nodes[index].parent {
                Some(parent) => index = parent,
                None => return false,
            }
        }
    }

    /// The widget that a click at this point goes to: the top one that can take a click.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.draw_order()
            .into_iter()
            .rev()
            .find(|&i| self.focusable(i) && self.shows_at(i, x, y))
    }

    /// The next widget for Tab, or the one before for Shift+Tab.
    pub fn next_focus(&self, from: Option<usize>, back: bool) -> Option<usize> {
        let order = self.focus_order();
        if order.is_empty() {
            return None;
        }
        let at = from.and_then(|f| order.iter().position(|&i| i == f));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
        };
        Some(order[next])
    }
}
