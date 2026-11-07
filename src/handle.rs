// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Typed handles to the controls of a dialog and the widgets of a window.

use std::fmt;
use std::marker::PhantomData;

/// A handle to a control of a [`Dialog`](crate::dialog::Dialog), or to a widget of a
/// [`Window`](crate::window::Window). The kind `K` decides which methods take it.
pub struct Id<K> {
    pub(crate) index: usize,
    kind: PhantomData<fn() -> K>,
}

impl<K> Id<K> {
    pub(crate) fn new(index: usize) -> Self {
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

/// A handle of any kind, as events name it. It compares equal to the [`Id`] of the same
/// control or widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnyId(pub(crate) usize);

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
