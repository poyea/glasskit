// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Widgets for a [`Window`](crate::window::Window): text, buttons, text boxes, cards, and
//! the rows, columns, and stacks that place them.
//!
//! Make a widget with its builder, give it to [`Window::add`](crate::window::Window::add),
//! and keep the [`Id`] that it gives. A container takes the ids of its children. Then
//! give the outer container to [`Window::set_root`](crate::window::Window::set_root).
//!
//! Sizes are in device-independent pixels (DIPs): one DIP is one pixel at 100% display
//! scale. The window scales everything to the display.

use std::fmt;
use std::marker::PhantomData;

pub(crate) mod tree;

pub use crate::handle::{AnyId, Id};

use crate::edit::Editor;
use tree::{Axis, Container, Content, Node};

/// The kinds of widgets. They are the type parameter of [`Id`].
pub mod kind {
    /// A [`Text`](super::Text).
    #[derive(Debug)]
    pub enum Text {}
    /// A [`Button`](super::Button).
    #[derive(Debug)]
    pub enum Button {}
    /// A [`NavItem`](super::NavItem).
    #[derive(Debug)]
    pub enum NavItem {}
    /// A [`Row`](super::Row).
    #[derive(Debug)]
    pub enum Row {}
    /// A [`Column`](super::Column).
    #[derive(Debug)]
    pub enum Column {}
    /// A [`Stack`](super::Stack).
    #[derive(Debug)]
    pub enum Stack {}
    /// A [`Card`](super::Card).
    #[derive(Debug)]
    pub enum Card {}
    /// A [`Scroll`](super::Scroll).
    #[derive(Debug)]
    pub enum Scroll {}
    /// A [`Spacer`](super::Spacer).
    #[derive(Debug)]
    pub enum Spacer {}
    /// A [`TextBox`](super::TextBox).
    #[derive(Debug)]
    pub enum TextBox {}
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::kind::Text {}
    impl Sealed for super::kind::Button {}
    impl Sealed for super::kind::NavItem {}
    impl Sealed for super::kind::TextBox {}
}

/// Widgets with a text that the app can read and change.
pub trait HasText: sealed::Sealed {}
impl HasText for kind::Text {}
impl HasText for kind::Button {}
impl HasText for kind::NavItem {}
impl HasText for kind::TextBox {}

/// The width or the height of a widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// The size of the content.
    Auto,
    /// Exactly this many DIPs.
    Fixed(f32),
    /// A share of the space that is left in the row or column. `Fill(2.0)` gets twice the
    /// space of `Fill(1.0)`. Across a row or column, it takes the full space.
    Fill(f32),
}

/// Where a widget goes across a row or a column, or in a stack, when it is smaller than the
/// space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// At the top or the left.
    Start,
    /// In the middle.
    Center,
    /// At the bottom or the right.
    End,
    /// Across the full space, if its length is [`Length::Auto`].
    Stretch,
}

/// Space inside the edges of a container, in DIPs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    /// The space at the left.
    pub left: f32,
    /// The space at the top.
    pub top: f32,
    /// The space at the right.
    pub right: f32,
    /// The space at the bottom.
    pub bottom: f32,
}

impl Insets {
    /// The same space on all four sides.
    pub const fn all(v: f32) -> Self {
        Self {
            left: v,
            top: v,
            right: v,
            bottom: v,
        }
    }

    /// `horizontal` at the left and the right, and `vertical` at the top and the bottom.
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }
}

/// The size and weight of a text, from the Windows type ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextStyle {
    /// 12 DIPs, for small notes.
    Caption,
    /// 14 DIPs, for most text.
    #[default]
    Body,
    /// 14 DIPs, semibold.
    BodyStrong,
    /// 20 DIPs, semibold, for the heading of a part.
    Subtitle,
    /// 28 DIPs, semibold, for the heading of a window.
    Title,
}

impl TextStyle {
    pub(crate) const ALL: [TextStyle; 5] = [
        TextStyle::Caption,
        TextStyle::Body,
        TextStyle::BodyStrong,
        TextStyle::Subtitle,
        TextStyle::Title,
    ];

    pub(crate) fn size(self) -> f32 {
        match self {
            TextStyle::Caption => 12.0,
            TextStyle::Body | TextStyle::BodyStrong => 14.0,
            TextStyle::Subtitle => 20.0,
            TextStyle::Title => 28.0,
        }
    }

    pub(crate) fn semibold(self) -> bool {
        !matches!(self, TextStyle::Caption | TextStyle::Body)
    }
}

/// A widget ready for [`Window::add`](crate::window::Window::add). The builders convert into
/// it.
pub struct Element<K> {
    pub(crate) node: Node,
    kind: PhantomData<fn() -> K>,
}

impl<K> Element<K> {
    fn new(node: Node) -> Self {
        Self {
            node,
            kind: PhantomData,
        }
    }
}

impl<K> fmt::Debug for Element<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Element").finish_non_exhaustive()
    }
}

macro_rules! sizes {
    () => {
        /// The width. The default is [`Length::Auto`].
        #[must_use]
        pub fn width(mut self, width: Length) -> Self {
            self.node.width = width;
            self
        }

        /// The height. The default is [`Length::Auto`].
        #[must_use]
        pub fn height(mut self, height: Length) -> Self {
            self.node.height = height;
            self
        }
    };
}

macro_rules! container {
    () => {
        /// Add a child at the end. A widget is in one container at a time: adding it to a
        /// second container moves it.
        #[must_use]
        pub fn child(mut self, child: impl Into<AnyId>) -> Self {
            self.container_mut().children.push(child.into().0);
            self
        }

        /// Add children at the end, in order.
        #[must_use]
        pub fn children<I: IntoIterator<Item = A>, A: Into<AnyId>>(mut self, children: I) -> Self {
            let list = &mut self.container_mut().children;
            list.extend(children.into_iter().map(|c| c.into().0));
            self
        }

        /// The space between two children, in DIPs.
        #[must_use]
        pub fn gap(mut self, gap: f32) -> Self {
            self.container_mut().gap = gap;
            self
        }

        /// The space inside the edges.
        #[must_use]
        pub fn padding(mut self, padding: Insets) -> Self {
            self.container_mut().padding = padding;
            self
        }

        /// Where the children go across, when they are smaller than the space.
        #[must_use]
        pub fn align(mut self, align: Align) -> Self {
            self.container_mut().align = align;
            self
        }

        fn container_mut(&mut self) -> &mut Container {
            self.node
                .container
                .as_mut()
                .expect("a container builder has a container")
        }
    };
}

macro_rules! element {
    ($builder:ident, $kind:ident) => {
        impl From<$builder> for Element<kind::$kind> {
            fn from(b: $builder) -> Self {
                Element::new(b.node)
            }
        }
    };
}

/// A text that wraps to the width that it gets.
#[derive(Debug)]
pub struct Text {
    node: Node,
}

impl Text {
    /// A text in [`TextStyle::Body`].
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            node: Node::leaf(Content::Text {
                text: text.into(),
                style: TextStyle::Body,
            }),
        }
    }

    /// The size and weight of the text.
    #[must_use]
    pub fn style(mut self, style: TextStyle) -> Self {
        if let Content::Text { style: s, .. } = &mut self.node.content {
            *s = style;
        }
        self
    }

    sizes!();
}

/// A button with a label. A click, Space, or Enter on it gives
/// [`WindowEvent::Clicked`](crate::window::WindowEvent::Clicked).
#[derive(Debug)]
pub struct Button {
    node: Node,
}

impl Button {
    /// A button with this label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            node: Node::leaf(Content::Button {
                label: label.into(),
                accent: false,
            }),
        }
    }

    /// Fill the button with the accent color of the user, for the main action of a window.
    #[must_use]
    pub fn accent(mut self) -> Self {
        if let Content::Button { accent, .. } = &mut self.node.content {
            *accent = true;
        }
        self
    }

    sizes!();
}

/// An item of a navigation list, for example at the left side of a window: a label that the
/// user picks to show one page of the window.
///
/// A click, Space, or Enter on it gives
/// [`WindowEvent::Clicked`](crate::window::WindowEvent::Clicked). The item does not select
/// itself: the app marks the item of the page that shows with
/// [`Window::set_selected`](crate::window::Window::set_selected), and shows the page with
/// [`Window::set_visible`](crate::window::Window::set_visible).
///
/// ```no_run
/// use glasskit::Ui;
/// use glasskit::events::{Event, EventLoop};
/// use glasskit::widget::{Column, Insets, Length, NavItem, Row, Stack, Text, TextStyle};
/// use glasskit::window::{Window, WindowEvent};
///
/// fn main() -> glasskit::Result<()> {
///     let ui = Ui::new()?;
///     let mut events: EventLoop<()> = EventLoop::new(&ui)?;
///     let mut window = Window::new(&ui, "Pages", 560.0, 360.0)?;
///     let home = window.add(NavItem::new("Home").selected(true));
///     let about = window.add(NavItem::new("About"));
///     let side = window.add(
///         Column::new()
///             .width(Length::Fixed(180.0))
///             .gap(4.0)
///             .child(home)
///             .child(about),
///     );
///     let home_page = window.add(Text::new("Home").style(TextStyle::Title));
///     let about_page = window.add(Text::new("About").style(TextStyle::Title));
///     window.set_visible(about_page, false);
///     let pages = window.add(
///         Stack::new()
///             .width(Length::Fill(1.0))
///             .child(home_page)
///             .child(about_page),
///     );
///     let root = window.add(
///         Row::new()
///             .padding(Insets::all(16.0))
///             .gap(16.0)
///             .child(side)
///             .child(pages),
///     );
///     window.set_root(root);
///     window.show();
///     loop {
///         if let Event::Window(id) = events.next(None)?
///             && id == window.id()
///         {
///             while let Some(event) = window.next_event() {
///                 match event {
///                     WindowEvent::Clicked(item) if item == home || item == about => {
///                         window.set_selected(home, item == home);
///                         window.set_selected(about, item == about);
///                         window.set_visible(home_page, item == home);
///                         window.set_visible(about_page, item == about);
///                     }
///                     WindowEvent::CloseRequested => return Ok(()),
///                     _ => {}
///                 }
///             }
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub struct NavItem {
    node: Node,
}

impl NavItem {
    /// An item with this label, not selected.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            node: Node::leaf(Content::NavItem {
                label: label.into(),
                selected: false,
            }),
        }
    }

    /// Mark the item as the one whose page shows.
    #[must_use]
    pub fn selected(mut self, on: bool) -> Self {
        if let Content::NavItem { selected, .. } = &mut self.node.content {
            *selected = on;
        }
        self
    }

    sizes!();
}

/// Children side by side, from the left.
#[derive(Debug)]
pub struct Row {
    node: Node,
}

impl Row {
    /// An empty row. Its children go in the middle across, and have no gap.
    pub fn new() -> Self {
        Self {
            node: Node::container(Content::Blank, Axis::Row, Align::Center),
        }
    }

    sizes!();
    container!();
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

/// Children one under the other, from the top.
#[derive(Debug)]
pub struct Column {
    node: Node,
}

impl Column {
    /// An empty column. Its children take the full width, and have no gap.
    pub fn new() -> Self {
        Self {
            node: Node::container(Content::Blank, Axis::Column, Align::Stretch),
        }
    }

    sizes!();
    container!();
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

/// Children on top of each other, the last on top.
#[derive(Debug)]
pub struct Stack {
    node: Node,
}

impl Stack {
    /// An empty stack. Its children take the full space.
    pub fn new() -> Self {
        Self {
            node: Node::container(Content::Blank, Axis::Stack, Align::Stretch),
        }
    }

    sizes!();
    container!();
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

/// A panel with a light fill and a thin border, around its children. Its children are on
/// top of each other, as in a [`Stack`], with 16 DIPs of padding.
#[derive(Debug)]
pub struct Card {
    node: Node,
}

impl Card {
    /// An empty card.
    pub fn new() -> Self {
        let mut node = Node::container(Content::Card, Axis::Stack, Align::Stretch);
        if let Some(c) = &mut node.container {
            c.padding = Insets::all(16.0);
        }
        Self { node }
    }

    sizes!();
    container!();
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

/// A column that the user scrolls up and down, when its children are higher than it: with
/// the mouse wheel, or with Tab to a widget that does not show. A thin bar at the right
/// shows where the view is.
///
/// Its children have the height that they want: [`Length::Fill`] gives them no more. By
/// default, the scroll fills the height that its row or column leaves.
#[derive(Debug)]
pub struct Scroll {
    node: Node,
}

impl Scroll {
    /// An empty scroll.
    pub fn new() -> Self {
        let mut node = Node::container(
            Content::Scroll {
                offset: 0.0,
                extent: 0.0,
            },
            Axis::Column,
            Align::Stretch,
        );
        node.height = Length::Fill(1.0);
        Self { node }
    }

    sizes!();
    container!();
}

impl Default for Scroll {
    fn default() -> Self {
        Self::new()
    }
}

/// Empty space. By default, it takes all the space that is left.
#[derive(Debug)]
pub struct Spacer {
    node: Node,
}

impl Spacer {
    /// Space that takes all that is left in its row or column.
    pub fn fill() -> Self {
        let mut node = Node::leaf(Content::Blank);
        node.width = Length::Fill(1.0);
        node.height = Length::Fill(1.0);
        Self { node }
    }

    /// Space of exactly this many DIPs in both directions.
    pub fn fixed(size: f32) -> Self {
        let mut node = Node::leaf(Content::Blank);
        node.width = Length::Fixed(size);
        node.height = Length::Fixed(size);
        Self { node }
    }
}

/// A box that the user types text in. It has one line, unless it is
/// [`multiline`](TextBox::multiline).
///
/// A change by the user gives [`WindowEvent::Changed`](crate::window::WindowEvent::Changed),
/// and Enter in a box with one line gives
/// [`WindowEvent::Submitted`](crate::window::WindowEvent::Submitted). Read and change the
/// text with [`Window::text`](crate::window::Window::text) and
/// [`Window::set_text`](crate::window::Window::set_text).
///
/// The box has the usual keys of Windows: the arrows (with Ctrl, by word; with Shift, they
/// select), Home, End, Backspace, Delete, and Ctrl+A, C, X, V, Z, and Y.
#[derive(Debug)]
pub struct TextBox {
    node: Node,
}

impl TextBox {
    /// An empty box with one line and no limit. Its width is 200 DIPs, unless the
    /// container stretches it.
    pub fn new() -> Self {
        Self {
            node: Node::leaf(Content::TextBox {
                edit: Editor::new(false, usize::MAX),
                placeholder: String::new(),
                scroll: 0.0,
            }),
        }
    }

    /// A hint that shows while the box is empty.
    #[must_use]
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        if let Content::TextBox { placeholder, .. } = &mut self.node.content {
            *placeholder = text.into();
        }
        self
    }

    /// Take many lines: Enter adds a line break. With a height of [`Length::Auto`], the box
    /// grows with its text; else it scrolls.
    #[must_use]
    pub fn multiline(mut self) -> Self {
        if let Content::TextBox { edit, .. } = &mut self.node.content {
            edit.set_multiline(true);
        }
        self
    }

    /// The most characters that the box takes.
    #[must_use]
    pub fn max_length(mut self, chars: usize) -> Self {
        if let Content::TextBox { edit, .. } = &mut self.node.content {
            edit.set_max_chars(chars);
        }
        self
    }

    sizes!();
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

element!(Text, Text);
element!(Button, Button);
element!(NavItem, NavItem);
element!(Row, Row);
element!(Column, Column);
element!(Stack, Stack);
element!(Card, Card);
element!(Scroll, Scroll);
element!(Spacer, Spacer);
element!(TextBox, TextBox);
