// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The widgets of a window as a tree, and its layout: measure, then arrange. It has no
//! window code. Text sizes come from a [`Measure`], so tests can use simple numbers.

use super::{Align, Insets, Length, TextStyle};
use crate::edit::Editor;

/// The inner space of a button around its label, in DIPs.
const BUTTON_PADDING: (f32, f32) = (12.0, 6.0);
const BUTTON_MIN: (f32, f32) = (64.0, 32.0);
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
    /// A panel behind the children.
    Card,
    TextBox {
        edit: Editor,
        placeholder: String,
        /// How far the text is scrolled: to the left with one line, up with many lines.
        scroll: f32,
    },
}

impl Content {
    pub fn interactive(&self) -> bool {
        matches!(self, Content::Button { .. } | Content::TextBox { .. })
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
        let index = self.nodes.len();
        let mut children: Vec<usize> = Vec::new();
        for &child in node.children() {
            if child < index && !children.contains(&child) {
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
        index
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
            None => intrinsic(&n.content, max_w, m),
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
        let gaps = c.gap * c.children.len().saturating_sub(1) as f32;
        let sizes = c
            .children
            .iter()
            .map(|&child| self.measure(child, inner_w, inner_h, m));
        let (w, h) = match c.axis {
            Axis::Row => sizes.fold((gaps, 0.0f32), |(w, h), (cw, ch)| (w + cw, h.max(ch))),
            Axis::Column => sizes.fold((0.0f32, gaps), |(w, h), (cw, ch)| (w.max(cw), h + ch)),
            Axis::Stack => sizes.fold((0.0f32, 0.0f32), |(w, h), (cw, ch)| (w.max(cw), h.max(ch))),
        };
        (w + p.left + p.right, h + p.top + p.bottom)
    }

    fn arrange(&mut self, index: usize, rect: Rect, m: &dyn Measure) {
        self.nodes[index].rect = rect;
        let Some(c) = self.nodes[index].container.clone() else {
            return;
        };
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
        if let Some(root) = self.root {
            self.walk(root, &mut out);
        }
        out
    }

    fn walk(&self, index: usize, out: &mut Vec<usize>) {
        out.push(index);
        for &child in self.nodes[index].children() {
            self.walk(child, out);
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

    /// Whether a widget is in the tree under the root.
    fn shown(&self, mut index: usize) -> bool {
        loop {
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
            .find(|&i| self.focusable(i) && self.nodes[i].rect.contains(x, y))
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

fn intrinsic(content: &Content, max_w: f32, m: &dyn Measure) -> (f32, f32) {
    match content {
        Content::Blank | Content::Card => (0.0, 0.0),
        Content::Text { text, style } => m.text(text, *style, max_w),
        Content::Button { label, .. } => {
            let (w, h) = m.label(label);
            (
                (w + 2.0 * BUTTON_PADDING.0).max(BUTTON_MIN.0),
                (h + 2.0 * BUTTON_PADDING.1).max(BUTTON_MIN.1),
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
