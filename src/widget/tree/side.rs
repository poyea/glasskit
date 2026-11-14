// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The side bar: its head, its width for each mode, and the button that steps the mode.

use super::*;

impl Tree {
    pub(super) fn add_side_head(&mut self, title: String) -> usize {
        let toggle = self.add(Node::leaf(Content::SideToggle));
        let title = self.add(Node::leaf(Content::Text {
            text: title,
            style: TextStyle::Subtitle,
        }));
        let mut head = Node::container(Content::Blank, Axis::Row, Align::Center);
        if let Some(c) = &mut head.container {
            c.children = vec![toggle, title];
        }
        self.add(head)
    }

    /// Give a side bar the width of its mode, and show the parts of its head that the mode
    /// has.
    pub(super) fn apply_side(&mut self, bar: usize) {
        let Content::SideBar {
            mode,
            modes,
            width,
            title,
        } = &self.nodes[bar].content
        else {
            return;
        };
        let expanded = *mode == SideMode::Expanded;
        let button = modes.len() > 1;
        let named = expanded && !title.is_empty();
        let padding = self.nodes[bar]
            .container
            .as_ref()
            .map_or(0.0, |c| c.padding.left + c.padding.right);
        let width = if expanded {
            *width
        } else {
            SIDE_COMPACT_WIDTH + padding
        };
        self.nodes[bar].width = Length::Fixed(width);
        let Some(&head) = self.nodes[bar].children().first() else {
            return;
        };
        self.nodes[head].visible = button || named;
        if let &[toggle, title] = self.nodes[head].children() {
            self.nodes[toggle].visible = button;
            self.nodes[title].visible = named;
        }
    }

    pub fn side_mode(&self, bar: usize) -> Option<SideMode> {
        match self.nodes.get(bar)?.content {
            Content::SideBar { mode, .. } => Some(mode),
            _ => None,
        }
    }

    pub fn set_side_mode(&mut self, bar: usize, to: SideMode) {
        if let Some(Content::SideBar { mode, .. }) = self.nodes.get_mut(bar).map(|n| &mut n.content)
        {
            *mode = to;
            self.apply_side(bar);
        }
    }

    /// Put the side bar of this button in its next mode. It gives the side bar.
    pub fn step_side(&mut self, toggle: usize) -> Option<usize> {
        let head = self.nodes.get(toggle)?.parent?;
        let bar = self.nodes[head].parent?;
        let Content::SideBar { mode, modes, .. } = &self.nodes[bar].content else {
            return None;
        };
        let at = modes.iter().position(|m| m == mode);
        let next = *modes.get(at.map_or(0, |i| (i + 1) % modes.len()))?;
        self.set_side_mode(bar, next);
        Some(bar)
    }

    /// Whether a widget is in a side bar that is not expanded.
    pub fn compact(&self, index: usize) -> bool {
        let mut at = self.nodes[index].parent;
        while let Some(parent) = at {
            if let Content::SideBar { mode, .. } = self.nodes[parent].content {
                return mode != SideMode::Expanded;
            }
            at = self.nodes[parent].parent;
        }
        false
    }
}
