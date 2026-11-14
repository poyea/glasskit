// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A kanban board for a [`Window`]: lanes side by side, each with a stack of cards.
//!
//! A card has a title and a body. The user drags a card to another place in its lane or to
//! another lane. A click on a card opens a small window that changes its title and its
//! body, and that can clone, pin, archive, or delete the card. The menu of a card, with the
//! right button of the mouse, has the same commands. Each lane has a button that adds a
//! card.
//!
//! - A card and a lane can have a color. The window of a card sets the color of the card,
//!   and the window of a lane sets the color of the lane.
//! - The board can have any number of lanes. The window of a lane, from the button with
//!   three dots, changes its name and deletes the lane. [`Board::add_lane_by_user`] asks
//!   the user for a new lane. The user cannot delete a [fixed](Board::set_lane_fixed) lane.
//! - A pinned card shows a pin, and stays above the cards of its lane that are not pinned.
//! - An archived card does not show, until the app asks for the archived cards with
//!   [`Board::set_show_archived`]. Then it is at the end of its lane.
//!
//! The app can also own a card: a [locked](CardData::locked) card has no window, and a
//! click on it gives [`BoardEvent::Clicked`]. The app changes, moves, and removes cards
//! with [`Board::set_card`], [`Board::move_card`], and [`Board::remove_card`].
//!
//! Make a [`Board`], put [`Board::widget`] in the window, and give each
//! [`WindowEvent`] of the window to [`Board::handle`]. The board keeps the cards: read them
//! with [`Board::cards`], for example to save them after a [`BoardEvent`].
//!
//! ```no_run
//! use glasskit::Ui;
//! use glasskit::events::{Event, EventLoop};
//! use glasskit::kanban::{Board, CardData};
//! use glasskit::widget::{Column, Insets};
//! use glasskit::window::{Window, WindowEvent};
//!
//! fn main() -> glasskit::Result<()> {
//!     let ui = Ui::new()?;
//!     let mut events: EventLoop<()> = EventLoop::new(&ui)?;
//!     let mut window = Window::new(&ui, "Board", 820.0, 520.0)?;
//!     let mut board = Board::new(&mut window, &["To do", "Doing", "Done"]);
//!     board.add_card(&mut window, 0, CardData::new("Write the plan", "One page"));
//!     let root = window.add(Column::new().padding(Insets::all(16.0)).child(board.widget()));
//!     window.set_root(root);
//!     window.show();
//!     loop {
//!         match events.next(None)? {
//!             Event::Window(id) if id == window.id() => {
//!                 while let Some(event) = window.next_event() {
//!                     if event == WindowEvent::CloseRequested {
//!                         return Ok(());
//!                     }
//!                     if let Some(change) = board.handle(&ui, &mut window, &event)? {
//!                         println!("{change:?}");
//!                     }
//!                 }
//!             }
//!             Event::Quit => return Ok(()),
//!             _ => {}
//!         }
//!     }
//! }
//! ```

use std::collections::HashMap;

use crate::dialog::{Bounds, Dialog, EditStyle, Event, Flow};
use crate::layout::{Column as Rows, Size};
use crate::tray::MenuItem;
use crate::widget::{
    Align, Button, Card, Column, Id, Insets, Length, Row, Scroll, Text, TextStyle, Tint, kind,
};
use crate::window::{Window, WindowEvent};
use crate::{Result, Ui};

const EDITOR_WIDTH: i32 = 560;
const EDITOR_HEIGHT: i32 = 346;
const LANE_EDITOR_WIDTH: i32 = 340;
const LANE_EDITOR_HEIGHT: i32 = 132;
const NO_COLOR: &str = "No color";
/// Characters of the icon font.
const ADD_ICON: &str = "\u{E710}";
const MORE_ICON: &str = "\u{E712}";
const PIN_ICON: &str = "\u{E718}";
const UNPIN_ICON: &str = "\u{E77A}";
const CLONE_ICON: &str = "\u{E8C8}";
const ARCHIVE_ICON: &str = "\u{E7B8}";
const RESTORE_ICON: &str = "\u{E7A7}";
const DELETE_ICON: &str = "\u{E74D}";

/// What a card of a [`Board`] holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardData {
    /// The title, on the first line of the card.
    pub title: String,
    /// The text below the title. It can be empty.
    pub body: String,
    /// The card stays above the cards of its lane that are not pinned.
    pub pinned: bool,
    /// The card shows only when the board shows the archived cards.
    pub archived: bool,
    /// The user cannot change the card: a click on it gives [`BoardEvent::Clicked`], and
    /// the board opens no window. The user can still drag it.
    pub locked: bool,
    /// The color of the card.
    pub tint: Option<Tint>,
}

impl CardData {
    /// A card with this title and body, not pinned and not archived.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ..Self::default()
        }
    }

    /// Pinned cards, then the other cards, then archived cards.
    fn rank(&self) -> u8 {
        match (self.archived, self.pinned) {
            (true, _) => 2,
            (false, true) => 0,
            (false, false) => 1,
        }
    }
}

/// The name of a card on a [`Board`]. It stays the same while the card moves and changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CardKey(u64);

/// What the user did on a [`Board`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BoardEvent {
    /// The user added a card.
    Added(CardKey),
    /// The user clicked a [locked](CardData::locked) card.
    Clicked(CardKey),
    /// The user changed the title or the body of a card.
    Edited(CardKey),
    /// The user dragged a card to another place. [`Board::place`] gives the place.
    Moved(CardKey),
    /// The user made a copy of a card.
    Cloned {
        /// The card that the user copied.
        from: CardKey,
        /// The new card, below the first one.
        to: CardKey,
    },
    /// The user pinned a card, or removed its pin. [`Board::card`] gives the state.
    Pinned(CardKey),
    /// The user archived a card, or took it back. [`Board::card`] gives the state.
    Archived(CardKey),
    /// The user added a lane, at the end.
    LaneAdded(usize),
    /// The user changed the name or the color of a lane. [`Board::lane_name`] and
    /// [`Board::lane_tint`] give them.
    LaneChanged(usize),
    /// The user deleted a lane and its cards. The lanes after it have an index that is one
    /// less than before.
    LaneRemoved {
        /// The index that the lane had.
        lane: usize,
        /// The name that the lane had.
        name: String,
        /// What its cards held, from the top.
        cards: Vec<CardData>,
    },
    /// The user deleted a card.
    Deleted {
        /// The key that the card had.
        key: CardKey,
        /// What the card held.
        card: CardData,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    key: CardKey,
    data: CardData,
}

/// The cards of a board in their lanes. It has no window code.
#[derive(Debug, Default)]
struct Model {
    lanes: Vec<Vec<Entry>>,
    next_key: u64,
}

impl Model {
    fn new(lanes: usize) -> Self {
        Self {
            lanes: vec![Vec::new(); lanes],
            next_key: 0,
        }
    }

    fn new_key(&mut self) -> CardKey {
        self.next_key += 1;
        CardKey(self.next_key)
    }

    /// The lane of a card, and its index among all cards of the lane.
    fn find(&self, key: CardKey) -> Option<(usize, usize)> {
        self.lanes.iter().enumerate().find_map(|(lane, cards)| {
            let at = cards.iter().position(|e| e.key == key)?;
            Some((lane, at))
        })
    }

    fn get(&self, key: CardKey) -> Option<&CardData> {
        let (lane, at) = self.find(key)?;
        Some(&self.lanes[lane][at].data)
    }

    /// Put the cards of a lane in the order of their ranks. Cards of one rank keep their
    /// order.
    fn sort(&mut self, lane: usize) {
        self.lanes[lane].sort_by_key(|e| e.data.rank());
    }

    fn add(&mut self, lane: usize, data: CardData) -> Option<CardKey> {
        if lane >= self.lanes.len() {
            return None;
        }
        let key = self.new_key();
        self.lanes[lane].push(Entry { key, data });
        self.sort(lane);
        Some(key)
    }

    /// The cards of a lane that show, from the top.
    fn shown(&self, lane: usize, archived: bool) -> Vec<CardKey> {
        self.lanes[lane]
            .iter()
            .filter(|e| archived || !e.data.archived)
            .map(|e| e.key)
            .collect()
    }

    fn update(&mut self, key: CardKey, change: impl FnOnce(&mut CardData)) {
        if let Some((lane, at)) = self.find(key) {
            change(&mut self.lanes[lane][at].data);
            self.sort(lane);
        }
    }

    /// Move a card to a lane. `index` is its place among the cards of the lane that show,
    /// counted without the card. The rank of the card can then move it: a pinned card
    /// stays above the cards that are not pinned.
    fn move_to(&mut self, key: CardKey, lane: usize, index: usize, archived: bool) -> bool {
        let Some((from, at)) = self.find(key) else {
            return false;
        };
        if lane >= self.lanes.len() {
            return false;
        }
        let entry = self.lanes[from].remove(at);
        let shown = self.shown(lane, archived);
        let cards = &mut self.lanes[lane];
        let place = |key: &CardKey| cards.iter().position(|e| e.key == *key);
        let at = match shown.get(index) {
            Some(next) => place(next),
            None => shown.last().and_then(place).map(|last| last + 1),
        };
        cards.insert(at.unwrap_or(0), entry);
        self.sort(lane);
        true
    }

    /// Make a copy of a card, below it.
    fn clone_card(&mut self, key: CardKey) -> Option<CardKey> {
        let (lane, at) = self.find(key)?;
        let data = self.lanes[lane][at].data.clone();
        let copy = self.new_key();
        self.lanes[lane].insert(at + 1, Entry { key: copy, data });
        Some(copy)
    }

    fn remove(&mut self, key: CardKey) -> Option<CardData> {
        let (lane, at) = self.find(key)?;
        Some(self.lanes[lane].remove(at).data)
    }

    /// Add an empty lane at the end, and give its index.
    fn add_lane(&mut self) -> usize {
        self.lanes.push(Vec::new());
        self.lanes.len() - 1
    }

    /// Remove a lane, and give what its cards held.
    fn remove_lane(&mut self, lane: usize) -> Option<Vec<CardData>> {
        if lane >= self.lanes.len() {
            return None;
        }
        let cards = self.lanes.remove(lane);
        Some(cards.into_iter().map(|e| e.data).collect())
    }
}

struct Lane {
    name: String,
    tint: Option<Tint>,
    /// The user cannot delete the lane.
    fixed: bool,
    panel: Id<kind::Card>,
    title: Id<kind::Text>,
    list: Id<kind::Scroll>,
    add: Id<kind::Button>,
    edit: Id<kind::Button>,
    count: Id<kind::Text>,
}

impl Lane {
    fn new(window: &mut Window, name: &str) -> Self {
        let title = window.add(
            Text::new(name)
                .style(TextStyle::BodyStrong)
                .width(Length::Fill(1.0)),
        );
        let count = window.add(Text::new("0").style(TextStyle::Caption));
        let edit = window.add(Button::icon(MORE_ICON));
        let add = window.add(Button::icon(ADD_ICON));
        let head = window.add(
            Row::new()
                .gap(8.0)
                .child(title)
                .child(count)
                .child(edit)
                .child(add),
        );
        // Space at the right keeps the cards clear of the scroll bar.
        let list = window.add(
            Scroll::new()
                .gap(8.0)
                .padding(Insets {
                    right: 10.0,
                    ..Insets::default()
                })
                .drop_target(),
        );
        let parts = window.add(Column::new().gap(8.0).child(head).child(list));
        let panel = window.add(
            Card::new()
                .width(Length::Fill(1.0))
                .padding(Insets::all(12.0))
                .child(parts),
        );
        Self {
            name: name.to_owned(),
            tint: None,
            fixed: false,
            panel,
            title,
            list,
            add,
            edit,
            count,
        }
    }
}

/// A kanban board in a [`Window`]. See the [module](self) for what the user can do on it.
pub struct Board {
    model: Model,
    lanes: Vec<Lane>,
    row: Id<kind::Row>,
    /// The widget of each card that shows.
    widgets: HashMap<CardKey, Id<kind::Card>>,
    /// Widgets of cards that do not show now, for the next cards.
    spare: Vec<Id<kind::Card>>,
    show_archived: bool,
}

impl std::fmt::Debug for Board {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Board")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl Board {
    /// A board with one empty lane for each name, in the widgets of `window`.
    pub fn new(window: &mut Window, lanes: &[&str]) -> Self {
        let lanes: Vec<Lane> = lanes.iter().map(|name| Lane::new(window, name)).collect();
        let row = window.add(
            Row::new()
                .gap(12.0)
                .align(Align::Stretch)
                .height(Length::Fill(1.0))
                .children(lanes.iter().map(|lane| lane.panel)),
        );
        Self {
            model: Model::new(lanes.len()),
            lanes,
            row,
            widgets: HashMap::new(),
            spare: Vec::new(),
            show_archived: false,
        }
    }

    /// The row of lanes. Put it in a container of the window. It takes all the height that
    /// it gets.
    pub fn widget(&self) -> Id<kind::Row> {
        self.row
    }

    /// The number of lanes.
    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    /// The name of a lane.
    pub fn lane_name(&self, lane: usize) -> Option<&str> {
        self.lanes.get(lane).map(|l| l.name.as_str())
    }

    /// Change the name of a lane.
    pub fn set_lane_name(&mut self, window: &mut Window, lane: usize, name: &str) {
        if let Some(lane) = self.lanes.get_mut(lane) {
            lane.name = name.to_owned();
            window.set_text(lane.title, name);
        }
    }

    /// Add an empty lane at the end, and give its index.
    pub fn add_lane(&mut self, window: &mut Window, name: &str) -> usize {
        self.lanes.push(Lane::new(window, name));
        self.show_lanes(window);
        self.model.add_lane()
    }

    /// Remove a lane and its cards, and give what the cards held, from the top. The lanes
    /// after it then have an index that is one less than before.
    pub fn remove_lane(&mut self, window: &mut Window, lane: usize) -> Option<Vec<CardData>> {
        let cards = self.model.remove_lane(lane)?;
        self.lanes.remove(lane);
        self.show_lanes(window);
        self.sync(window);
        Some(cards)
    }

    /// Whether the user cannot delete a lane.
    pub fn lane_fixed(&self, lane: usize) -> bool {
        self.lanes.get(lane).is_some_and(|l| l.fixed)
    }

    /// Keep a lane: the user can change its name and its color, but cannot delete it. The
    /// app can still remove it with [`Board::remove_lane`].
    pub fn set_lane_fixed(&mut self, lane: usize, fixed: bool) {
        if let Some(lane) = self.lanes.get_mut(lane) {
            lane.fixed = fixed;
        }
    }

    /// Ask the user for the name and the color of a new lane, in a small window, and wait
    /// until it closes. The new lane goes at the end, and the result is
    /// [`BoardEvent::LaneAdded`].
    pub fn add_lane_by_user(&mut self, ui: &Ui, window: &mut Window) -> Result<Option<BoardEvent>> {
        let Some(LaneEdit::Save { name, tint }) = lane_editor(ui, window, None)? else {
            return Ok(None);
        };
        let lane = self.add_lane(window, &name);
        self.set_lane_tint(window, lane, tint);
        Ok(Some(BoardEvent::LaneAdded(lane)))
    }

    fn show_lanes(&self, window: &mut Window) {
        window.set_children(self.row, self.lanes.iter().map(|lane| lane.panel));
    }

    /// The color of a lane.
    pub fn lane_tint(&self, lane: usize) -> Option<Tint> {
        self.lanes.get(lane).and_then(|l| l.tint)
    }

    /// Give a lane a color, or take its color away.
    pub fn set_lane_tint(&mut self, window: &mut Window, lane: usize, tint: Option<Tint>) {
        if let Some(lane) = self.lanes.get_mut(lane) {
            lane.tint = tint;
            window.set_tint(lane.panel, tint);
        }
    }

    /// Add a card at the end of a lane, below the cards of its kind: pinned, not pinned, or
    /// archived. It gives `None` if the board has no such lane.
    pub fn add_card(
        &mut self,
        window: &mut Window,
        lane: usize,
        card: CardData,
    ) -> Option<CardKey> {
        let key = self.model.add(lane, card)?;
        self.sync(window);
        Some(key)
    }

    /// Change what a card holds. It gives false if the board has no such card.
    pub fn set_card(&mut self, window: &mut Window, key: CardKey, card: CardData) -> bool {
        let found = self.model.get(key).is_some();
        self.model.update(key, |old| *old = card);
        self.sync(window);
        found
    }

    /// Move a card to a lane. `index` is its place among the cards of the lane that show,
    /// counted without the card: 0 is the top, and a large number is the end. A pinned card
    /// stays above the cards that are not pinned.
    pub fn move_card(
        &mut self,
        window: &mut Window,
        key: CardKey,
        lane: usize,
        index: usize,
    ) -> bool {
        let moved = self.model.move_to(key, lane, index, self.show_archived);
        self.sync(window);
        moved
    }

    /// Remove a card, and give what it held.
    pub fn remove_card(&mut self, window: &mut Window, key: CardKey) -> Option<CardData> {
        let card = self.model.remove(key);
        self.sync(window);
        card
    }

    /// What a card holds.
    pub fn card(&self, key: CardKey) -> Option<&CardData> {
        self.model.get(key)
    }

    /// The lane of a card, and its index among all cards of the lane, also the archived
    /// ones.
    pub fn place(&self, key: CardKey) -> Option<(usize, usize)> {
        self.model.find(key)
    }

    /// All cards of a lane from the top, also the archived ones.
    pub fn cards(&self, lane: usize) -> Vec<(CardKey, CardData)> {
        self.model.lanes.get(lane).map_or_else(Vec::new, |cards| {
            cards.iter().map(|e| (e.key, e.data.clone())).collect()
        })
    }

    /// Whether the archived cards show.
    pub fn show_archived(&self) -> bool {
        self.show_archived
    }

    /// Show the archived cards at the end of their lanes, or hide them.
    pub fn set_show_archived(&mut self, window: &mut Window, show: bool) {
        self.show_archived = show;
        self.sync(window);
    }

    /// Give an event of the window to the board. If the event is for the board, the board
    /// changes, and the result tells what the user did. A click on a card or on a button of
    /// a lane opens a small window, and a click with the right button on a card opens a
    /// menu. This call returns when the window or the menu closes.
    pub fn handle(
        &mut self,
        ui: &Ui,
        window: &mut Window,
        event: &WindowEvent,
    ) -> Result<Option<BoardEvent>> {
        match event {
            WindowEvent::Clicked(id) => {
                if let Some(lane) = self.lanes.iter().position(|lane| *id == lane.add) {
                    return self.add_by_user(ui, window, lane);
                }
                if let Some(lane) = self.lanes.iter().position(|lane| *id == lane.edit) {
                    return self.edit_lane(ui, window, lane);
                }
                match self.key_of(|widget| *id == widget) {
                    Some(key) => self.edit(ui, window, key),
                    None => Ok(None),
                }
            }
            WindowEvent::ContextRequested(id) => match self.key_of(|widget| *id == widget) {
                Some(key) => self.menu(ui, window, key),
                None => Ok(None),
            },
            WindowEvent::Dropped {
                item,
                target,
                index,
            } => {
                let key = self.key_of(|widget| *item == widget);
                let lane = self.lanes.iter().position(|lane| *target == lane.list);
                let (Some(key), Some(lane)) = (key, lane) else {
                    return Ok(None);
                };
                self.model.move_to(key, lane, *index, self.show_archived);
                self.sync(window);
                Ok(Some(BoardEvent::Moved(key)))
            }
            _ => Ok(None),
        }
    }

    /// Open the window of a card, as a click on the card does, and wait until it closes. A
    /// [locked](CardData::locked) card gives [`BoardEvent::Clicked`] and opens no window.
    pub fn edit(
        &mut self,
        ui: &Ui,
        window: &mut Window,
        key: CardKey,
    ) -> Result<Option<BoardEvent>> {
        let Some(card) = self.model.get(key).cloned() else {
            return Ok(None);
        };
        if card.locked {
            return Ok(Some(BoardEvent::Clicked(key)));
        }
        let Some(edit) = editor(ui, window, &card, false)? else {
            return Ok(None);
        };
        Ok(self.apply(window, key, edit.action, Some(edit)))
    }

    /// Show the menu of a card, and do what the user picks. A locked card has no menu.
    fn menu(&mut self, ui: &Ui, window: &mut Window, key: CardKey) -> Result<Option<BoardEvent>> {
        let Some(card) = self.model.get(key).filter(|card| !card.locked) else {
            return Ok(None);
        };
        let pin = if card.pinned { "Unpin" } else { "Pin" };
        let archive = if card.archived { "Restore" } else { "Archive" };
        // `None` is the command that opens the window of the card.
        let items = [
            MenuItem::command(None, "Edit"),
            MenuItem::command(Some(Action::Pin), pin),
            MenuItem::command(Some(Action::Clone), "Clone"),
            MenuItem::command(Some(Action::Archive), archive),
            MenuItem::Separator,
            MenuItem::command(Some(Action::Delete), "Delete"),
        ];
        match window.menu(&items) {
            None => Ok(None),
            Some(None) => self.edit(ui, window, key),
            Some(Some(action)) => Ok(self.apply(window, key, action, None)),
        }
    }

    /// Do an action on a card. With `edit`, the card first takes its texts and its color.
    fn apply(
        &mut self,
        window: &mut Window,
        key: CardKey,
        action: Action,
        edit: Option<Edit>,
    ) -> Option<BoardEvent> {
        let event = match action {
            Action::Delete => self
                .model
                .remove(key)
                .map(|card| BoardEvent::Deleted { key, card }),
            Action::Save | Action::Clone | Action::Pin | Action::Archive => {
                self.model.get(key)?;
                self.model.update(key, |card| {
                    if let Some(edit) = edit {
                        card.title = edit.title;
                        card.body = edit.body;
                        card.tint = edit.tint;
                    }
                    card.pinned ^= action == Action::Pin;
                    card.archived ^= action == Action::Archive;
                });
                match action {
                    Action::Clone => self
                        .model
                        .clone_card(key)
                        .map(|to| BoardEvent::Cloned { from: key, to }),
                    Action::Pin => Some(BoardEvent::Pinned(key)),
                    Action::Archive => Some(BoardEvent::Archived(key)),
                    Action::Save | Action::Delete => Some(BoardEvent::Edited(key)),
                }
            }
        };
        self.sync(window);
        event
    }

    fn add_by_user(
        &mut self,
        ui: &Ui,
        window: &mut Window,
        lane: usize,
    ) -> Result<Option<BoardEvent>> {
        let Some(edit) = editor(ui, window, &CardData::default(), true)? else {
            return Ok(None);
        };
        let card = CardData {
            tint: edit.tint,
            ..CardData::new(edit.title, edit.body)
        };
        let key = self.add_card(window, lane, card);
        Ok(key.map(BoardEvent::Added))
    }

    /// Open the window of a lane, and wait until it closes.
    fn edit_lane(
        &mut self,
        ui: &Ui,
        window: &mut Window,
        lane: usize,
    ) -> Result<Option<BoardEvent>> {
        let cards = &self.model.lanes[lane];
        let now = LaneNow {
            name: &self.lanes[lane].name,
            tint: self.lanes[lane].tint,
            fixed: self.lanes[lane].fixed,
            cards: cards.len(),
            locked: cards.iter().any(|e| e.data.locked),
        };
        match lane_editor(ui, window, Some(now))? {
            None => Ok(None),
            Some(LaneEdit::Save { name, tint }) => {
                self.set_lane_name(window, lane, &name);
                self.set_lane_tint(window, lane, tint);
                Ok(Some(BoardEvent::LaneChanged(lane)))
            }
            Some(LaneEdit::Delete) => {
                let name = self.lanes[lane].name.clone();
                let cards = self.remove_lane(window, lane).unwrap_or_default();
                Ok(Some(BoardEvent::LaneRemoved { lane, name, cards }))
            }
        }
    }

    fn key_of(&self, is: impl Fn(Id<kind::Card>) -> bool) -> Option<CardKey> {
        self.widgets
            .iter()
            .find(|(_, widget)| is(**widget))
            .map(|(key, _)| *key)
    }

    /// Make the widgets agree with the cards.
    fn sync(&mut self, window: &mut Window) {
        let lists: Vec<Vec<CardKey>> = (0..self.lanes.len())
            .map(|lane| self.model.shown(lane, self.show_archived))
            .collect();
        let hidden: Vec<CardKey> = self
            .widgets
            .keys()
            .filter(|key| !lists.iter().any(|list| list.contains(key)))
            .copied()
            .collect();
        for key in hidden {
            self.spare.extend(self.widgets.remove(&key));
        }
        for (lane, keys) in self.lanes.iter().zip(&lists) {
            let mut widgets = Vec::with_capacity(keys.len());
            for key in keys {
                let widget = match self.widgets.get(key) {
                    Some(widget) => *widget,
                    None => {
                        let widget = self.spare.pop().unwrap_or_else(|| card_widget(window));
                        self.widgets.insert(*key, widget);
                        widget
                    }
                };
                if let Some(card) = self.model.get(*key) {
                    window.set_card_text(widget, &shown_title(card), &card.body);
                    window.set_pinned(widget, card.pinned && !card.archived);
                    window.set_tint(widget, card.tint);
                }
                widgets.push(widget);
            }
            window.set_children(lane.list, widgets);
            window.set_text(lane.count, &keys.len().to_string());
        }
    }
}

fn card_widget(window: &mut Window) -> Id<kind::Card> {
    // The space at the right is for the pin.
    window.add(
        Card::new()
            .title("")
            .body("")
            .clickable()
            .draggable()
            .padding(Insets {
                left: 14.0,
                top: 10.0,
                right: 30.0,
                bottom: 10.0,
            }),
    )
}

fn shown_title(card: &CardData) -> String {
    if card.archived {
        format!("{} (archived)", card.title)
    } else {
        card.title.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Save,
    Clone,
    Pin,
    Archive,
    Delete,
}

/// What the user chose in the window of a card, with the texts as they were then.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    action: Action,
    title: String,
    body: String,
    tint: Option<Tint>,
}

fn color_names() -> Vec<&'static str> {
    let names = Tint::ALL.iter().map(|tint| tint.name());
    std::iter::once(NO_COLOR).chain(names).collect()
}

/// The index of a color in [`color_names`].
fn color_index(tint: Option<Tint>) -> usize {
    tint.and_then(|tint| Tint::ALL.iter().position(|t| *t == tint))
        .map_or(0, |at| at + 1)
}

fn color_at(index: Option<usize>) -> Option<Tint> {
    Tint::ALL.get(index?.checked_sub(1)?).copied()
}

/// A lane as its window shows it at the start.
struct LaneNow<'a> {
    name: &'a str,
    tint: Option<Tint>,
    fixed: bool,
    /// The number of its cards, also the archived ones.
    cards: usize,
    /// It has a card that the user cannot delete.
    locked: bool,
}

/// What the user chose in the window of a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LaneEdit {
    Save { name: String, tint: Option<Tint> },
    Delete,
}

/// The question before a lane with cards goes.
fn delete_question(cards: usize) -> String {
    match cards {
        1 => "Delete this lane and its card?".to_owned(),
        n => format!("Delete this lane and its {n} cards, also the archived ones?"),
    }
}

/// Show the window of a lane, and wait until it closes. `None` means that the user changed
/// nothing. Without a lane, the window is for a new lane.
fn lane_editor(ui: &Ui, owner: &Window, lane: Option<LaneNow>) -> Result<Option<LaneEdit>> {
    let (width, height) = (LANE_EDITOR_WIDTH, LANE_EDITOR_HEIGHT);
    let title = lane.as_ref().map_or("New lane", |lane| lane.name);
    let mut d = Dialog::new(ui, title, width, height)?;
    d.set_owner(owner);
    let mut rows = Rows::new(Bounds::new(0, 0, width, height).inset(16), 8);
    let [label, field] = rows.next(28).columns([Size::Fixed(60), Size::Fill(1)], 8);
    d.add_label("Name", label)?;
    let name = d.add_edit(EditStyle::default(), field)?;
    let [label, pick] = rows.next(26).columns([Size::Fixed(60), Size::Fill(1)], 8);
    d.add_label("Color", label)?;
    let color = d.add_drop_down(pick)?;
    d.set_items(color, &color_names());
    d.set_selected(color, Some(color_index(lane.as_ref().and_then(|l| l.tint))));
    d.set_text(name, lane.as_ref().map_or("", |l| l.name));
    let sizes = [
        Size::Fixed(80),
        Size::Fill(1),
        Size::Fixed(80),
        Size::Fixed(80),
    ];
    let [delete, _, cancel, save] = rows.next(30).columns(sizes, 8);
    let delete = match &lane {
        Some(lane) if !lane.fixed => Some(d.add_button("Delete", delete)?),
        _ => None,
    };
    let cancel = d.add_button("Cancel", cancel)?;
    let save = d.add_button("Save", save)?;
    d.focus(name);

    let mut chosen = None;
    d.run(|d, event| {
        match event {
            Event::Close => return Flow::Close,
            Event::Clicked(id) if id == cancel => return Flow::Close,
            Event::Clicked(id) if delete.is_some_and(|delete| id == delete) => {
                let Some(lane) = &lane else {
                    return Flow::Continue;
                };
                if lane.locked {
                    let why = "This lane has cards that you cannot delete. \
                               Move them to another lane first.";
                    d.warn(title, why);
                    return Flow::Continue;
                }
                if lane.cards > 0 && !d.confirm(title, &delete_question(lane.cards)) {
                    return Flow::Continue;
                }
                chosen = Some(LaneEdit::Delete);
                return Flow::Close;
            }
            Event::Clicked(id) if id == save => {}
            Event::Submit => {}
            _ => return Flow::Continue,
        }
        let text = d.text(name).trim().to_owned();
        if text.is_empty() {
            d.warn(title, "Give the lane a name.");
            return Flow::Continue;
        }
        chosen = Some(LaneEdit::Save {
            name: text,
            tint: color_at(d.selected(color)),
        });
        Flow::Close
    })?;
    Ok(chosen)
}

/// The title of the window of a card.
fn editor_title(card: &CardData, new: bool) -> &str {
    match card.title.as_str() {
        _ if new => "New card",
        "" => "Card",
        title => title,
    }
}

/// Show the window of a card, and wait until it closes. `None` means that the user changed
/// nothing. A new card has only Save and Cancel.
fn editor(ui: &Ui, owner: &Window, card: &CardData, new: bool) -> Result<Option<Edit>> {
    let name = editor_title(card, new);
    let mut d = Dialog::new(ui, name, EDITOR_WIDTH, EDITOR_HEIGHT)?;
    d.set_owner(owner);
    let mut rows = Rows::new(Bounds::new(0, 0, EDITOR_WIDTH, EDITOR_HEIGHT).inset(16), 8);
    d.add_label("Title", rows.next(20))?;
    let title = d.add_edit(EditStyle::default(), rows.next(28))?;
    d.add_label("Body", rows.next(20))?;
    let style = EditStyle {
        multiline: true,
        ..EditStyle::default()
    };
    let body = d.add_edit(style, rows.next(150))?;
    let [label, pick, _] = rows
        .next(26)
        .columns([Size::Fixed(60), Size::Fixed(160), Size::Fill(1)], 8);
    d.add_label("Color", label)?;
    let color = d.add_drop_down(pick)?;
    d.set_items(color, &color_names());
    d.set_selected(color, Some(color_index(card.tint)));
    d.set_text(title, &card.title);
    d.set_text(body, &card.body.replace('\n', "\r\n"));

    let sizes = [
        Size::Fixed(32),
        Size::Fixed(32),
        Size::Fixed(32),
        Size::Fixed(32),
        Size::Fill(1),
        Size::Fixed(80),
        Size::Fixed(80),
    ];
    let [pin, clone, archive, delete, _, cancel, save] = rows.next(30).columns(sizes, 8);
    let mut actions = Vec::new();
    if !new {
        let (pin_icon, pin_name) = if card.pinned {
            (UNPIN_ICON, "Unpin")
        } else {
            (PIN_ICON, "Pin")
        };
        let (archive_icon, archive_name) = if card.archived {
            (RESTORE_ICON, "Restore")
        } else {
            (ARCHIVE_ICON, "Archive")
        };
        for (icon, name, bounds, action) in [
            (pin_icon, pin_name, pin, Action::Pin),
            (CLONE_ICON, "Clone", clone, Action::Clone),
            (archive_icon, archive_name, archive, Action::Archive),
            (DELETE_ICON, "Delete", delete, Action::Delete),
        ] {
            actions.push((d.add_icon_button(icon, name, bounds)?, action));
        }
    }
    let cancel = d.add_button("Cancel", cancel)?;
    actions.push((d.add_button("Save", save)?, Action::Save));
    d.focus(title);

    let mut chosen = None;
    d.run(|d, event| {
        let action = match event {
            Event::Close => return Flow::Close,
            Event::Clicked(id) if id == cancel => return Flow::Close,
            Event::Submit => Action::Save,
            Event::Clicked(id) => match actions.iter().find(|(button, _)| id == *button) {
                Some((_, action)) => *action,
                None => return Flow::Continue,
            },
            Event::Changed(_) => return Flow::Continue,
        };
        let edit = Edit {
            action,
            title: d.text(title).trim().to_owned(),
            body: d.text(body).replace("\r\n", "\n").trim_end().to_owned(),
            tint: color_at(d.selected(color)),
        };
        if edit.title.is_empty() && action != Action::Delete {
            d.warn(name, "Give the card a title.");
            return Flow::Continue;
        }
        chosen = Some(edit);
        Flow::Close
    })?;
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(model: &Model, lane: usize, archived: bool) -> Vec<String> {
        model
            .shown(lane, archived)
            .into_iter()
            .map(|key| model.get(key).unwrap().title.clone())
            .collect()
    }

    /// Lane 0 has the cards A, B, and C, and lane 1 has the card D.
    fn model() -> (Model, [CardKey; 4]) {
        let mut model = Model::new(2);
        let keys = [(0, "A"), (0, "B"), (0, "C"), (1, "D")]
            .map(|(lane, title)| model.add(lane, CardData::new(title, "")).unwrap());
        (model, keys)
    }

    #[test]
    fn a_card_moves_in_its_lane_and_to_another_lane() {
        let (mut model, [a, _, c, _]) = model();
        assert!(model.move_to(a, 0, 2, false));
        assert_eq!(titles(&model, 0, false), ["B", "C", "A"]);
        assert!(model.move_to(c, 1, 0, false));
        assert_eq!(titles(&model, 0, false), ["B", "A"]);
        assert_eq!(titles(&model, 1, false), ["C", "D"]);
        assert!(model.move_to(a, 1, 9, false));
        assert_eq!(titles(&model, 1, false), ["C", "D", "A"]);
        assert!(!model.move_to(a, 5, 0, false));
        assert_eq!(model.find(a), Some((1, 2)));
        assert!(model.add(5, CardData::default()).is_none());
    }

    #[test]
    fn a_pinned_card_stays_above_the_others() {
        let (mut model, [a, b, c, d]) = model();
        model.update(c, |card| card.pinned = true);
        assert_eq!(titles(&model, 0, false), ["C", "A", "B"]);
        model.move_to(c, 0, 2, false);
        assert_eq!(titles(&model, 0, false), ["C", "A", "B"]);
        model.move_to(b, 0, 0, false);
        assert_eq!(titles(&model, 0, false), ["C", "B", "A"]);
        model.update(a, |card| card.pinned = true);
        assert_eq!(titles(&model, 0, false), ["C", "A", "B"]);
        model.move_to(c, 1, 1, false);
        assert_eq!(titles(&model, 1, false), ["C", "D"]);
        model.update(c, |card| card.pinned = false);
        assert_eq!(titles(&model, 1, false), ["C", "D"]);
        assert!(model.get(d).is_some());
    }

    #[test]
    fn an_archived_card_shows_only_on_request() {
        let (mut model, [a, b, _, d]) = model();
        model.update(a, |card| card.archived = true);
        assert_eq!(titles(&model, 0, false), ["B", "C"]);
        assert_eq!(titles(&model, 0, true), ["B", "C", "A"]);
        // A card that goes to the end of the cards that show is above the archived card.
        model.move_to(d, 0, 2, false);
        assert_eq!(titles(&model, 0, true), ["B", "C", "D", "A"]);
        model.update(b, |card| card.archived = true);
        model.update(a, |card| card.archived = false);
        assert_eq!(titles(&model, 0, true), ["C", "D", "A", "B"]);
        // A lane with only archived cards takes a card at its top.
        let (mut model, [a, _, _, d]) = self::model();
        model.update(d, |card| card.archived = true);
        model.move_to(a, 1, 0, false);
        assert_eq!(titles(&model, 1, true), ["A", "D"]);
    }

    #[test]
    fn a_card_is_cloned_below_itself_and_deleted() {
        let (mut model, [a, b, _, _]) = model();
        model.update(a, |card| card.body = "text".into());
        let copy = model.clone_card(a).unwrap();
        assert_ne!(copy, a);
        assert_eq!(titles(&model, 0, false), ["A", "A", "B", "C"]);
        assert_eq!(model.get(copy), model.get(a));
        assert_eq!(model.remove(b).unwrap().title, "B");
        assert_eq!(model.remove(b), None);
        assert_eq!(titles(&model, 0, false), ["A", "A", "C"]);
        assert_eq!(model.clone_card(b), None);
    }

    #[test]
    fn a_lane_is_added_at_the_end_and_removed_with_its_cards() {
        let (mut model, [a, _, _, d]) = model();
        assert_eq!(model.add_lane(), 2);
        assert!(model.move_to(a, 2, 0, false));
        assert_eq!(titles(&model, 2, false), ["A"]);
        let cards = model.remove_lane(0).unwrap();
        let removed: Vec<&str> = cards.iter().map(|card| card.title.as_str()).collect();
        assert_eq!(removed, ["B", "C"]);
        assert_eq!(model.find(d), Some((0, 0)));
        assert_eq!(model.find(a), Some((1, 0)));
        assert_eq!(model.remove_lane(2), None);
    }

    #[test]
    fn the_window_of_a_card_has_the_title_of_the_card() {
        let card = CardData::new("Write the plan", "");
        assert_eq!(editor_title(&card, false), "Write the plan");
        assert_eq!(editor_title(&CardData::default(), false), "Card");
        assert_eq!(editor_title(&card, true), "New card");
    }

    #[test]
    fn the_question_counts_the_cards_of_the_lane() {
        assert_eq!(delete_question(1), "Delete this lane and its card?");
        assert!(delete_question(3).contains("its 3 cards"));
    }

    #[test]
    fn each_color_has_a_place_in_the_list() {
        assert_eq!(color_names().len(), Tint::ALL.len() + 1);
        assert_eq!(color_at(Some(color_index(None))), None);
        for tint in Tint::ALL {
            assert_eq!(color_at(Some(color_index(Some(tint)))), Some(tint));
            assert_eq!(Tint::from_name(&tint.name().to_lowercase()), Some(tint));
        }
        assert_eq!(color_at(None), None);
        assert_eq!(color_at(Some(99)), None);
        assert_eq!(Tint::from_name("clear"), None);
    }

    #[test]
    fn an_archived_card_says_so_in_its_title() {
        let mut card = CardData::new("Plan", "");
        assert_eq!(shown_title(&card), "Plan");
        card.archived = true;
        assert_eq!(shown_title(&card), "Plan (archived)");
    }
}
