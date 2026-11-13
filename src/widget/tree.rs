// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The widgets of a window as a tree, and its layout: measure, then arrange. It has no
//! window code. Text sizes come from a [`Measure`], so tests can use simple numbers.

use super::{Align, Insets, Length, SideMode, TextStyle};
use crate::edit::Editor;

/// The inner space of a button around its label, in DIPs.
const BUTTON_PADDING: (f32, f32) = (12.0, 6.0);
const BUTTON_MIN: (f32, f32) = (64.0, 32.0);
/// The space in a navigation item at the left and the right of its label, in DIPs. The
/// left space has the selection mark in it.
pub(crate) const NAV_ITEM_PADDING: (f32, f32) = (16.0, 12.0);
const NAV_ITEM_HEIGHT: f32 = 36.0;
/// The width of the icon of a navigation item, and the space between it and the label, in
/// DIPs.
pub(crate) const NAV_ICON: (f32, f32) = (16.0, 16.0);
/// The width of a side bar that is not expanded, and of its button, in DIPs. The icon of a
/// navigation item is in the middle of this width.
pub(crate) const SIDE_COMPACT_WIDTH: f32 = 48.0;
/// The inner space of a text box around its text, in DIPs. It includes the border.
pub(crate) const TEXT_BOX_PADDING: (f32, f32) = (11.0, 6.0);
/// The width of a text box with [`Length::Auto`], in DIPs.
const TEXT_BOX_WIDTH: f32 = 200.0;
const TEXT_BOX_MIN_HEIGHT: f32 = 32.0;

/// A rectangle in DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
        }
    }

    fn inset(self, i: Insets) -> Self {
        Rect::new(
            self.x + i.left,
            self.y + i.top,
            self.w - i.left - i.right,
            self.h - i.top - i.bottom,
        )
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// The part that is in both rectangles. It is empty if they do not touch.
    pub fn intersect(self, other: Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.w).min(other.x + other.w);
        let bottom = (self.y + self.h).min(other.y + other.h);
        Rect::new(x, y, right - x, bottom - y)
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

/// What a widget shows.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    /// Nothing: a spacer, or a container without a look.
    Blank,
    Text {
        text: String,
        style: TextStyle,
    },
    Button {
        label: String,
        accent: bool,
    },
    /// An item of a navigation list.
    NavItem {
        label: String,
        icon: Option<String>,
        selected: bool,
    },
    /// A side bar. Its first child is its head: a row with its button and its title.
    SideBar {
        mode: SideMode,
        /// The modes that the button steps through.
        modes: Vec<SideMode>,
        /// The width when it is expanded, in DIPs.
        width: f32,
        title: String,
    },
    /// The button of a side bar.
    SideToggle,
    /// A panel behind the children.
    Card,
    /// A column that scrolls up and down when its children are higher than it.
    Scroll {
        /// How far the children are scrolled up, in DIPs.
        offset: f32,
        /// The height of the children with the padding, at the last layout.
        extent: f32,
    },
    TextBox {
        edit: Editor,
        placeholder: String,
        /// How far the text is scrolled: to the left with one line, up with many lines.
        scroll: f32,
    },
}

impl Content {
    pub fn interactive(&self) -> bool {
        matches!(
            self,
            Content::Button { .. }
                | Content::NavItem { .. }
                | Content::SideToggle
                | Content::TextBox { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Axis {
    Row,
    Column,
    Stack,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Container {
    pub axis: Axis,
    pub gap: f32,
    pub padding: Insets,
    pub align: Align,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub content: Content,
    pub container: Option<Container>,
    pub width: Length,
    pub height: Length,
    pub enabled: bool,
    /// A widget that is not visible, and all widgets in it, take no space and no input.
    pub visible: bool,
    pub parent: Option<usize>,
    /// Where the last layout put the widget.
    pub rect: Rect,
}

impl Node {
    pub fn leaf(content: Content) -> Self {
        Self {
            content,
            container: None,
            width: Length::Auto,
            height: Length::Auto,
            enabled: true,
            visible: true,
            parent: None,
            rect: Rect::default(),
        }
    }

    pub fn container(content: Content, axis: Axis, align: Align) -> Self {
        Self {
            container: Some(Container {
                axis,
                gap: 0.0,
                padding: Insets::default(),
                align,
                children: Vec::new(),
            }),
            ..Self::leaf(content)
        }
    }

    pub fn children(&self) -> &[usize] {
        self.container.as_ref().map_or(&[], |c| &c.children)
    }
}

/// The size of a text in DIPs, when it wraps at `max_width`.
pub(crate) trait Measure {
    fn text(&self, text: &str, style: TextStyle, max_width: f32) -> (f32, f32);
    /// The size of a button label, on one line.
    fn label(&self, label: &str) -> (f32, f32);
}

#[derive(Debug, Default)]
pub(crate) struct Tree {
    pub nodes: Vec<Node>,
    pub root: Option<usize>,
}

impl Tree {
    /// Add a widget. Its children move out of any container that held them before.
    pub fn add(&mut self, node: Node) -> usize {
        let limit = self.nodes.len();
        let head = match &node.content {
            Content::SideBar { title, .. } => Some(self.add_side_head(title.clone())),
            _ => None,
        };
        let index = self.nodes.len();
        let mut children: Vec<usize> = head.into_iter().collect();
        for &child in node.children() {
            if child < limit && !children.contains(&child) {
                children.push(child);
            }
        }
        self.nodes.push(node);
        if let Some(c) = &mut self.nodes[index].container {
            c.children.clone_from(&children);
        }
        for child in children {
            if let Some(old) = self.nodes[child].parent.replace(index)
                && let Some(c) = &mut self.nodes[old].container
            {
                c.children.retain(|&x| x != child);
            }
            if self.root == Some(child) {
                self.root = None;
            }
        }
        self.apply_side(index);
        index
    }

    fn add_side_head(&mut self, title: String) -> usize {
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
    fn apply_side(&mut self, bar: usize) {
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

    /// Whether a widget takes space in its container: it is visible, and it is not below
    /// the head of a collapsed side bar.
    fn present(&self, index: usize) -> bool {
        let n = &self.nodes[index];
        n.visible
            && n.parent.is_none_or(|p| match self.nodes[p].content {
                Content::SideBar {
                    mode: SideMode::Collapsed,
                    ..
                } => self.nodes[p].children().first() == Some(&index),
                _ => true,
            })
    }

    pub fn get(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Node> {
        self.nodes.get_mut(index)
    }

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

    fn measure_container(
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

    /// Place the children of a scroll as a column at their full height, moved up by the
    /// offset. The offset stays between the top and the end of the children.
    fn arrange_scroll(
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
    fn shows_at(&self, index: usize, x: f32, y: f32) -> bool {
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

    fn arrange_column(&mut self, c: &Container, inner: Rect, m: &dyn Measure) {
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
        self.nodes
            .get(index)
            .is_some_and(|n| n.enabled && n.content.interactive() && self.shown(index))
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

fn intrinsic(content: &Content, compact: bool, max_w: f32, m: &dyn Measure) -> (f32, f32) {
    match content {
        Content::Blank | Content::Card | Content::Scroll { .. } | Content::SideBar { .. } => {
            (0.0, 0.0)
        }
        Content::SideToggle => (SIDE_COMPACT_WIDTH, NAV_ITEM_HEIGHT),
        Content::NavItem { .. } if compact => (SIDE_COMPACT_WIDTH, NAV_ITEM_HEIGHT),
        Content::Text { text, style } => m.text(text, *style, max_w),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Each character is 8 DIPs wide and a line is 20 DIPs high. Text wraps by character.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&self, text: &str, _: TextStyle, max_width: f32) -> (f32, f32) {
            let chars = text.chars().count() as f32;
            let per_line = (max_width / 8.0).floor().max(1.0);
            let lines = (chars / per_line).ceil().max(1.0);
            ((chars.min(per_line)) * 8.0, lines * 20.0)
        }

        fn label(&self, label: &str) -> (f32, f32) {
            (label.chars().count() as f32 * 8.0, 20.0)
        }
    }

    fn text(s: &str) -> Node {
        Node::leaf(Content::Text {
            text: s.into(),
            style: TextStyle::Body,
        })
    }

    fn button(s: &str) -> Node {
        Node::leaf(Content::Button {
            label: s.into(),
            accent: false,
        })
    }

    fn with(mut node: Node, children: &[usize]) -> Node {
        node.container.as_mut().unwrap().children = children.to_vec();
        node
    }

    fn scroll(children: &[usize]) -> Node {
        let mut node = with(
            Node::container(
                Content::Scroll {
                    offset: 0.0,
                    extent: 0.0,
                },
                Axis::Column,
                Align::Stretch,
            ),
            children,
        );
        node.height = Length::Fill(1.0);
        node
    }

    /// Five buttons of 32 DIPs in a scroll of 100 DIPs: 160 DIPs, so 60 do not show.
    fn five_in_a_scroll() -> (Tree, usize, Vec<usize>) {
        let mut t = Tree::default();
        let buttons: Vec<usize> = (0..5).map(|i| t.add(button(&format!("B{i}")))).collect();
        let s = t.add(scroll(&buttons));
        t.root = Some(s);
        t.layout(200.0, 100.0, &Fixed);
        (t, s, buttons)
    }

    #[test]
    fn a_scroll_moves_its_children_and_stays_in_range() {
        let (mut t, s, b) = five_in_a_scroll();
        assert_eq!(t.nodes[b[4]].rect, Rect::new(0.0, 128.0, 200.0, 32.0));
        assert_eq!(
            t.nodes[s].content,
            Content::Scroll {
                offset: 0.0,
                extent: 160.0
            }
        );

        t.scroll_by(s, 40.0);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[b[0]].rect.y, -40.0);
        assert_eq!(t.nodes[b[4]].rect.y, 88.0);

        t.scroll_by(s, 500.0);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[b[4]].rect.y, 68.0);
        t.scroll_by(s, -500.0);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[b[0]].rect.y, 0.0);
    }

    #[test]
    fn a_scroll_cuts_what_is_outside_it() {
        let (t, s, b) = five_in_a_scroll();
        assert_eq!(t.clip(b[3]), Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
        assert_eq!(t.clip(s), None);
        // The fourth button is at 96 to 128: only its top shows, and only there it is hit.
        assert_eq!(t.hit(5.0, 98.0), Some(b[3]));
        assert_eq!(t.hit(5.0, 110.0), None);
    }

    #[test]
    fn the_wheel_finds_a_scroll_that_can_move() {
        let (mut t, s, _) = five_in_a_scroll();
        assert_eq!(t.scroll_at(5.0, 50.0, 10.0), Some(s));
        assert_eq!(t.scroll_at(5.0, 50.0, -10.0), None);
        assert_eq!(t.scroll_at(5.0, 150.0, 10.0), None);
        t.scroll_by(s, 60.0);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.scroll_at(5.0, 50.0, 10.0), None);
        assert_eq!(t.scroll_at(5.0, 50.0, -10.0), Some(s));
    }

    #[test]
    fn reveal_scrolls_a_widget_into_view() {
        let (mut t, s, b) = five_in_a_scroll();
        assert!(t.reveal(b[4]));
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[b[4]].rect.y + 32.0, 100.0);
        assert!(!t.reveal(b[4]));
        assert!(t.reveal(b[0]));
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[b[0]].rect.y, 0.0);
        assert!(!t.reveal(s));
    }

    #[test]
    fn a_short_scroll_does_not_move() {
        let mut t = Tree::default();
        let ok = t.add(button("OK"));
        let s = t.add(scroll(&[ok]));
        t.root = Some(s);
        t.layout(200.0, 100.0, &Fixed);
        t.scroll_by(s, 50.0);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[ok].rect.y, 0.0);
        assert_eq!(t.scroll_at(5.0, 5.0, 10.0), None);
    }

    #[test]
    fn a_hidden_widget_takes_no_space_and_no_gap() {
        let mut t = Tree::default();
        let a = t.add(text("one"));
        let b = t.add(button("OK"));
        let c = t.add(text("two"));
        let mut col = with(
            Node::container(Content::Blank, Axis::Column, Align::Stretch),
            &[a, b, c],
        );
        col.container.as_mut().unwrap().gap = 10.0;
        let col = t.add(col);
        t.root = Some(col);
        t.nodes[b].visible = false;
        assert_eq!(t.measure(col, 200.0, 100.0, &Fixed), (24.0, 50.0));
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[c].rect, Rect::new(0.0, 30.0, 200.0, 20.0));
        assert_eq!(t.draw_order(), [col, a, c]);
        assert!(!t.focusable(b));
        assert_eq!(t.hit(5.0, 25.0), None);

        t.nodes[b].visible = true;
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[c].rect, Rect::new(0.0, 72.0, 200.0, 20.0));
        assert_eq!(t.hit(5.0, 35.0), Some(b));
    }

    #[test]
    fn a_hidden_container_hides_its_widgets() {
        let mut t = Tree::default();
        let ok = t.add(button("OK"));
        let page = t.add(with(
            Node::container(Content::Blank, Axis::Column, Align::Stretch),
            &[ok],
        ));
        let root = t.add(with(
            Node::container(Content::Blank, Axis::Stack, Align::Stretch),
            &[page],
        ));
        t.root = Some(root);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.hit(5.0, 5.0), Some(ok));
        t.nodes[page].visible = false;
        assert_eq!(t.hit(5.0, 5.0), None);
        assert!(!t.focusable(ok));
        assert_eq!(t.next_focus(None, false), None);
    }

    #[test]
    fn a_navigation_item_has_space_for_its_mark() {
        let mut t = Tree::default();
        let item = t.add(nav("Inbox", None));
        assert_eq!(t.measure(item, 200.0, 100.0, &Fixed), (68.0, 36.0));
        assert!(t.nodes[item].content.interactive());
        let with_icon = t.add(nav("Inbox", Some("i")));
        assert_eq!(t.measure(with_icon, 200.0, 100.0, &Fixed), (100.0, 36.0));
    }

    fn nav(label: &str, icon: Option<&str>) -> Node {
        Node::leaf(Content::NavItem {
            label: label.into(),
            icon: icon.map(Into::into),
            selected: false,
        })
    }

    /// A side bar of 200 DIPs with two items, at the left of a page, in a window of 400 by
    /// 300 DIPs.
    fn side_bar(modes: &[SideMode], title: &str) -> (Tree, usize, [usize; 2], usize) {
        let mut t = Tree::default();
        let items = [t.add(nav("Home", Some("h"))), t.add(nav("Mail", None))];
        let bar = t.add(with(
            Node::container(
                Content::SideBar {
                    mode: SideMode::Expanded,
                    modes: modes.to_vec(),
                    width: 200.0,
                    title: title.into(),
                },
                Axis::Column,
                Align::Stretch,
            ),
            &items,
        ));
        let mut page = text("page");
        page.width = Length::Fill(1.0);
        let page = t.add(page);
        let row = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Stretch),
            &[bar, page],
        ));
        t.root = Some(row);
        t.layout(400.0, 300.0, &Fixed);
        (t, bar, items, page)
    }

    fn toggle_of(t: &Tree, bar: usize) -> usize {
        let head = t.nodes[bar].children()[0];
        t.nodes[head].children()[0]
    }

    #[test]
    fn a_side_bar_has_a_width_for_each_mode() {
        let (mut t, bar, items, page) = side_bar(&SideMode::ALL, "App");
        let toggle = toggle_of(&t, bar);
        assert_eq!(t.nodes[bar].rect.w, 200.0);
        assert_eq!(t.nodes[toggle].rect, Rect::new(0.0, 0.0, 48.0, 36.0));
        assert_eq!(t.nodes[items[0]].rect, Rect::new(0.0, 36.0, 200.0, 36.0));
        assert_eq!(t.nodes[page].rect.x, 200.0);

        t.set_side_mode(bar, SideMode::Icons);
        t.layout(400.0, 300.0, &Fixed);
        assert_eq!(t.nodes[items[1]].rect, Rect::new(0.0, 72.0, 48.0, 36.0));
        assert_eq!(t.nodes[page].rect.x, 48.0);
        assert_eq!(t.focus_order(), [toggle, items[0], items[1]]);

        t.set_side_mode(bar, SideMode::Collapsed);
        t.layout(400.0, 300.0, &Fixed);
        assert_eq!(t.nodes[bar].rect.w, 48.0);
        assert_eq!(t.focus_order(), [toggle]);
        assert_eq!(t.hit(5.0, 40.0), None);
        assert!(!t.shown(items[0]));
    }

    #[test]
    fn the_button_of_a_side_bar_steps_through_its_modes() {
        let (mut t, bar, _, _) = side_bar(&[SideMode::Expanded, SideMode::Collapsed], "");
        let toggle = toggle_of(&t, bar);
        assert_eq!(t.step_side(toggle), Some(bar));
        assert_eq!(t.side_mode(bar), Some(SideMode::Collapsed));
        t.step_side(toggle);
        assert_eq!(t.side_mode(bar), Some(SideMode::Expanded));
        // From a mode that is not in the list, the button goes to the first one.
        t.set_side_mode(bar, SideMode::Icons);
        t.step_side(toggle);
        assert_eq!(t.side_mode(bar), Some(SideMode::Expanded));
    }

    #[test]
    fn a_side_bar_with_one_mode_has_no_button() {
        let (t, _, items, _) = side_bar(&[SideMode::Expanded], "");
        assert_eq!(t.focus_order(), items);
        assert_eq!(t.nodes[items[0]].rect.y, 0.0);
        let (t, bar, items, _) = side_bar(&[], "App");
        let head = t.nodes[bar].children()[0];
        assert!(t.shown(head));
        assert_eq!(t.focus_order(), items);
    }

    #[test]
    fn a_column_stacks_and_stretches() {
        let mut t = Tree::default();
        let a = t.add(text("hello"));
        let b = t.add(button("OK"));
        let col = t.add(with(
            Node::container(Content::Blank, Axis::Column, Align::Stretch),
            &[a, b],
        ));
        t.root = Some(col);
        t.layout(200.0, 100.0, &Fixed);
        assert_eq!(t.nodes[a].rect, Rect::new(0.0, 0.0, 200.0, 20.0));
        assert_eq!(t.nodes[b].rect, Rect::new(0.0, 20.0, 200.0, 32.0));
    }

    #[test]
    fn fills_share_what_is_left_by_weight() {
        let mut t = Tree::default();
        let ok = t.add(button("OK"));
        let mut one = Node::leaf(Content::Blank);
        one.width = Length::Fill(1.0);
        let one = t.add(one);
        let mut two = Node::leaf(Content::Blank);
        two.width = Length::Fill(2.0);
        let two = t.add(two);
        let mut row = with(
            Node::container(Content::Blank, Axis::Row, Align::Center),
            &[one, ok, two],
        );
        row.container.as_mut().unwrap().gap = 8.0;
        let row = t.add(row);
        t.root = Some(row);
        t.layout(400.0, 100.0, &Fixed);
        // 400 - 64 (button) - 16 (gaps) = 320, split 1:2.
        assert!((t.nodes[one].rect.w - 106.666_67).abs() < 0.01);
        assert_eq!(t.nodes[ok].rect.x, t.nodes[one].rect.w + 8.0);
        assert!((t.nodes[two].rect.w - 213.333_33).abs() < 0.01);
        // Centered across.
        assert_eq!(t.nodes[ok].rect.y, 34.0);
    }

    #[test]
    fn text_wraps_to_the_column_width() {
        let mut t = Tree::default();
        let long = t.add(text("abcdefghij"));
        let col = t.add(with(
            Node::container(Content::Blank, Axis::Column, Align::Stretch),
            &[long],
        ));
        t.root = Some(col);
        t.layout(40.0, 200.0, &Fixed);
        assert_eq!(t.nodes[long].rect.h, 40.0);
    }

    #[test]
    fn padding_and_gap_are_inside_and_between() {
        let mut t = Tree::default();
        let a = t.add(text("a"));
        let b = t.add(text("b"));
        let mut col = with(
            Node::container(Content::Card, Axis::Column, Align::Start),
            &[a, b],
        );
        let c = col.container.as_mut().unwrap();
        c.padding = Insets::all(10.0);
        c.gap = 5.0;
        let col = t.add(col);
        t.root = Some(col);
        t.layout(100.0, 100.0, &Fixed);
        assert_eq!(t.nodes[a].rect, Rect::new(10.0, 10.0, 8.0, 20.0));
        assert_eq!(t.nodes[b].rect, Rect::new(10.0, 35.0, 8.0, 20.0));
        assert_eq!(t.measure(col, 100.0, 100.0, &Fixed), (28.0, 65.0));
    }

    #[test]
    fn tab_goes_through_enabled_buttons_and_wraps() {
        let mut t = Tree::default();
        let a = t.add(button("A"));
        let mut off = button("off");
        off.enabled = false;
        let off = t.add(off);
        let b = t.add(button("B"));
        let row = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Center),
            &[a, off, b],
        ));
        t.root = Some(row);
        assert_eq!(t.focus_order(), [a, b]);
        assert_eq!(t.next_focus(None, false), Some(a));
        assert_eq!(t.next_focus(Some(a), false), Some(b));
        assert_eq!(t.next_focus(Some(b), false), Some(a));
        assert_eq!(t.next_focus(Some(a), true), Some(b));
    }

    #[test]
    fn a_click_goes_to_the_button_under_it() {
        let mut t = Tree::default();
        let a = t.add(button("A"));
        let row = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Start),
            &[a],
        ));
        t.root = Some(row);
        t.layout(300.0, 100.0, &Fixed);
        assert_eq!(t.hit(10.0, 10.0), Some(a));
        assert_eq!(t.hit(200.0, 10.0), None);
    }

    #[test]
    fn a_child_moves_to_its_new_container() {
        let mut t = Tree::default();
        let a = t.add(text("a"));
        let first = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Start),
            &[a],
        ));
        let second = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Start),
            &[a],
        ));
        assert!(t.nodes[first].children().is_empty());
        assert_eq!(t.nodes[second].children(), [a]);
        assert_eq!(t.nodes[a].parent, Some(second));
    }

    #[test]
    fn a_child_listed_twice_is_in_its_container_once() {
        let mut t = Tree::default();
        let a = t.add(text("a"));
        let b = t.add(text("b"));
        let col = t.add(with(
            Node::container(Content::Blank, Axis::Column, Align::Start),
            &[a, b, a],
        ));
        assert_eq!(t.nodes[col].children(), [a, b]);
    }

    fn text_box(text: &str, multiline: bool) -> Node {
        let mut edit = Editor::new(multiline, usize::MAX);
        edit.set_text(text);
        Node::leaf(Content::TextBox {
            edit,
            placeholder: String::new(),
            scroll: 0.0,
        })
    }

    #[test]
    fn a_text_box_takes_focus_and_grows_with_many_lines() {
        let mut t = Tree::default();
        let one = t.add(text_box("abcdefghij", false));
        let many = t.add(text_box("abcdefghij", true));
        let col = t.add(with(
            Node::container(Content::Blank, Axis::Column, Align::Start),
            &[one, many],
        ));
        t.root = Some(col);
        t.layout(60.0, 300.0, &Fixed);
        assert_eq!(t.focus_order(), [one, many]);
        // One line: 20 + 2 * 6 padding, at least 32.
        assert_eq!(t.nodes[one].rect, Rect::new(0.0, 0.0, 60.0, 32.0));
        // 60 - 2 * 11 = 38 DIPs for text: 4 characters a line, so 3 lines.
        assert_eq!(t.nodes[many].rect.h, 72.0);
    }

    #[test]
    fn a_widget_outside_the_root_takes_no_focus() {
        let mut t = Tree::default();
        let a = t.add(button("A"));
        let b = t.add(button("B"));
        let row = t.add(with(
            Node::container(Content::Blank, Axis::Row, Align::Start),
            &[a],
        ));
        t.root = Some(row);
        assert_eq!(t.focus_order(), [a]);
        assert!(!t.focusable(b));
    }
}
