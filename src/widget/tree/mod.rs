// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The widgets of a window as a tree, and its layout: measure, then arrange. It has no
//! window code. Text sizes come from a [`Measure`], so tests can use simple numbers.

use super::{Align, Insets, Length, SideMode, TextStyle, Tint};
use crate::edit::Editor;

mod card;
mod drag;
mod focus;
mod layout;
mod scroll;
mod side;
#[cfg(test)]
mod tests;

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
/// The space between the title and the body of a card, in DIPs.
const CARD_TEXT_GAP: f32 = 4.0;

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
        /// The label is a character of the icon font.
        icon: bool,
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
    /// A panel behind the children. A card with a text has its title and its body as its
    /// first two children.
    Card {
        text: Option<(String, String)>,
        pinned: bool,
        tint: Option<Tint>,
    },
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
    pub fn card() -> Self {
        Content::Card {
            text: None,
            pinned: false,
            tint: None,
        }
    }

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
    /// A click on the widget gives an event, and Tab moves to it.
    pub clickable: bool,
    pub draggable: bool,
    pub drop_target: bool,
}

/// Where a dragged widget goes if the user drops it now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DropPlace {
    pub target: usize,
    /// The index among the children of the target, without the dragged widget.
    pub index: usize,
    /// False when the widget is at this place already.
    pub changes: bool,
    /// A line at the place, between two children of the target.
    pub line: Option<Rect>,
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
            clickable: false,
            draggable: false,
            drop_target: false,
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
    pub fn add(&mut self, mut node: Node) -> usize {
        let limit = self.nodes.len();
        let mut children: Vec<usize> = match &node.content {
            Content::SideBar { title, .. } => vec![self.add_side_head(title.clone())],
            Content::Card {
                text: Some((title, body)),
                ..
            } => self.add_card_text(title.clone(), body.clone()),
            _ => Vec::new(),
        };
        if let (Content::Card { text: Some(_), .. }, Some(c)) = (&node.content, &mut node.container)
        {
            c.axis = Axis::Column;
            if c.gap == 0.0 {
                c.gap = CARD_TEXT_GAP;
            }
        }
        let index = self.nodes.len();
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

    /// The number of children at the start of a container that are parts of the container
    /// itself: the head of a side bar, and the texts of a card.
    fn own_start(&self, index: usize) -> usize {
        match &self.nodes[index].content {
            Content::SideBar { .. } => 1,
            Content::Card { text: Some(_), .. } => 2,
            _ => 0,
        }
    }

    /// Give a container these children, in this order, in place of the ones that it has.
    /// They move out of the containers that held them before.
    pub fn set_children(&mut self, parent: usize, children: &[usize]) {
        if self.nodes.get(parent).is_none_or(|n| n.container.is_none()) {
            return;
        }
        let mut list: Vec<usize> = Vec::new();
        for &child in children {
            if child < self.nodes.len() && !list.contains(&child) && !self.within(parent, child) {
                list.push(child);
            }
        }
        let start = self
            .own_start(parent)
            .min(self.nodes[parent].children().len());
        let old: Vec<usize> = self.nodes[parent].children()[start..].to_vec();
        for child in old {
            self.nodes[child].parent = None;
        }
        for &child in &list {
            if let Some(from) = self.nodes[child].parent.replace(parent)
                && let Some(c) = &mut self.nodes[from].container
            {
                c.children.retain(|&x| x != child);
            }
            if self.root == Some(child) {
                self.root = None;
            }
        }
        if let Some(c) = &mut self.nodes[parent].container {
            c.children.truncate(start);
            c.children.extend(list);
        }
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
}
