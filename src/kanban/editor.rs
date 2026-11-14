// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The small windows that change a card or a lane.

use super::CardData;
use crate::dialog::{Bounds, Dialog, EditStyle, Event, Flow};
use crate::layout::{Column as Rows, Size};
use crate::widget::Tint;
use crate::window::Window;
use crate::{Result, Ui};

const EDITOR_WIDTH: i32 = 560;
const EDITOR_HEIGHT: i32 = 346;
const LANE_EDITOR_WIDTH: i32 = 340;
const LANE_EDITOR_HEIGHT: i32 = 132;
const NO_COLOR: &str = "No color";
const PIN_ICON: &str = "\u{E718}";
const UNPIN_ICON: &str = "\u{E77A}";
const CLONE_ICON: &str = "\u{E8C8}";
const ARCHIVE_ICON: &str = "\u{E7B8}";
const RESTORE_ICON: &str = "\u{E7A7}";
const DELETE_ICON: &str = "\u{E74D}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Save,
    Clone,
    Pin,
    Archive,
    Delete,
}

/// What the user chose in the window of a card, with the texts as they were then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Edit {
    pub(super) action: Action,
    pub(super) title: String,
    pub(super) body: String,
    pub(super) tint: Option<Tint>,
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
pub(super) struct LaneNow<'a> {
    pub(super) name: &'a str,
    pub(super) tint: Option<Tint>,
    pub(super) fixed: bool,
    /// The number of its cards, also the archived ones.
    pub(super) cards: usize,
    /// It has a card that the user cannot delete.
    pub(super) locked: bool,
}

/// What the user chose in the window of a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LaneEdit {
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
pub(super) fn lane_editor(
    ui: &Ui,
    owner: &Window,
    lane: Option<LaneNow>,
) -> Result<Option<LaneEdit>> {
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
pub(super) fn editor(ui: &Ui, owner: &Window, card: &CardData, new: bool) -> Result<Option<Edit>> {
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
}
