// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The public types of a dialog: bounds, typed handles of controls, and events.

use super::*;

/// A position and size in pixels at 100% display scale. The dialog scales them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The left edge, from the left of the client area.
    pub x: i32,
    /// The top edge, from the top of the client area.
    pub y: i32,
    /// The width.
    pub width: i32,
    /// The height.
    pub height: i32,
}

impl Bounds {
    /// Bounds with this top-left corner and size.
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// The kinds of controls. They are the type parameter of [`Id`], and they decide which
/// [`Dialog`] methods an id can go to.
pub mod kind {
    /// A text that the user cannot change.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Label {}
    /// A card with a heading, behind the controls that are on it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Group {}
    /// A box for text, on one line or on many.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Edit {}
    /// A closed list that opens when the user clicks it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DropDown {}
    /// An open list of items.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum List {}
    /// A check box with a text.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Check {}
    /// A push button.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Button {}
    /// A picker for where a window shows on the screen. See [`Dialog::add_placement_picker`].
    ///
    /// [`Dialog::add_placement_picker`]: super::Dialog::add_placement_picker
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PlacementPicker {}
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::kind::Label {}
    impl Sealed for super::kind::Edit {}
    impl Sealed for super::kind::DropDown {}
    impl Sealed for super::kind::List {}
}

/// The kinds of controls that show a text that the app can read and change.
pub trait HasText: sealed::Sealed {}

impl HasText for kind::Label {}

impl HasText for kind::Edit {}

/// The kinds of controls that hold a list of items, of which the user can select one.
pub trait HasItems: sealed::Sealed {}

impl HasItems for kind::DropDown {}

impl HasItems for kind::List {}

/// A control in a [`Dialog`]. The kind `K` decides which methods take it.
pub struct Id<K> {
    pub(super) index: usize,
    pub(super) kind: PhantomData<fn() -> K>,
}

impl<K> Id<K> {
    pub(super) fn new(index: usize) -> Self {
        Self {
            index,
            kind: PhantomData,
        }
    }
}

impl<K> Clone for Id<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Id<K> {}

impl<K> PartialEq for Id<K> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<K> Eq for Id<K> {}

impl<K> std::hash::Hash for Id<K> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}

impl<K> fmt::Debug for Id<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.index)
    }
}

/// A control of any kind, as an [`Event`] names it. Compare it with an [`Id`] of any kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyId(pub(super) usize);

impl<K> From<Id<K>> for AnyId {
    fn from(id: Id<K>) -> Self {
        AnyId(id.index)
    }
}

impl<K> PartialEq<Id<K>> for AnyId {
    fn eq(&self, other: &Id<K>) -> bool {
        self.0 == other.index
    }
}

impl<K> PartialEq<AnyId> for Id<K> {
    fn eq(&self, other: &AnyId) -> bool {
        self.index == other.0
    }
}

/// How an edit box takes text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditStyle {
    /// The box has many lines. Enter starts a new line, and a scroll bar shows when the text
    /// is longer than the box.
    pub multiline: bool,
    /// The box takes only the digits 0 to 9.
    pub digits_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What the user did. [`Dialog::run`] gives events to its handler one at a time.
pub enum Event {
    /// A button or a check box was clicked.
    Clicked(AnyId),
    /// The user changed the text of an edit box or the selection of a list.
    /// Changes made through [`Dialog`] methods do not send this event.
    Changed(AnyId),
    /// The user pressed Enter outside a multiline edit box.
    Submit,
    /// The user pressed Esc or the close button.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What the handler of [`Dialog::run`] wants next.
pub enum Flow {
    /// Keep the dialog open and wait for the next event.
    Continue,
    /// Close the dialog. [`Dialog::run`] then returns [`Outcome::Closed`].
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How [`Dialog::run`] ended.
pub enum Outcome {
    /// The handler returned [`Flow::Close`].
    Closed,
    /// The thread got WM_QUIT. The dialog posts WM_QUIT again for the caller.
    Quit,
}
