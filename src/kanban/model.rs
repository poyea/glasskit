// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The cards of a board in their lanes. It has no window code.

use super::{CardData, CardKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) key: CardKey,
    pub(super) data: CardData,
}

#[derive(Debug, Default)]
pub(super) struct Model {
    pub(super) lanes: Vec<Vec<Entry>>,
    next_key: u64,
}

impl Model {
    pub(super) fn new(lanes: usize) -> Self {
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
    pub(super) fn find(&self, key: CardKey) -> Option<(usize, usize)> {
        self.lanes.iter().enumerate().find_map(|(lane, cards)| {
            let at = cards.iter().position(|e| e.key == key)?;
            Some((lane, at))
        })
    }

    pub(super) fn get(&self, key: CardKey) -> Option<&CardData> {
        let (lane, at) = self.find(key)?;
        Some(&self.lanes[lane][at].data)
    }

    /// Put the cards of a lane in the order of their ranks. Cards of one rank keep their
    /// order.
    fn sort(&mut self, lane: usize) {
        self.lanes[lane].sort_by_key(|e| e.data.rank());
    }

    pub(super) fn add(&mut self, lane: usize, data: CardData) -> Option<CardKey> {
        if lane >= self.lanes.len() {
            return None;
        }
        let key = self.new_key();
        self.lanes[lane].push(Entry { key, data });
        self.sort(lane);
        Some(key)
    }

    /// The cards of a lane that show, from the top.
    pub(super) fn shown(&self, lane: usize, archived: bool) -> Vec<CardKey> {
        self.lanes[lane]
            .iter()
            .filter(|e| archived || !e.data.archived)
            .map(|e| e.key)
            .collect()
    }

    pub(super) fn update(&mut self, key: CardKey, change: impl FnOnce(&mut CardData)) {
        if let Some((lane, at)) = self.find(key) {
            change(&mut self.lanes[lane][at].data);
            self.sort(lane);
        }
    }

    /// Move a card to a lane. `index` is its place among the cards of the lane that show,
    /// counted without the card. The rank of the card can then move it: a pinned card
    /// stays above the cards that are not pinned.
    pub(super) fn move_to(
        &mut self,
        key: CardKey,
        lane: usize,
        index: usize,
        archived: bool,
    ) -> bool {
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
    pub(super) fn clone_card(&mut self, key: CardKey) -> Option<CardKey> {
        let (lane, at) = self.find(key)?;
        let data = self.lanes[lane][at].data.clone();
        let copy = self.new_key();
        self.lanes[lane].insert(at + 1, Entry { key: copy, data });
        Some(copy)
    }

    pub(super) fn remove(&mut self, key: CardKey) -> Option<CardData> {
        let (lane, at) = self.find(key)?;
        Some(self.lanes[lane].remove(at).data)
    }

    /// Add an empty lane at the end, and give its index.
    pub(super) fn add_lane(&mut self) -> usize {
        self.lanes.push(Vec::new());
        self.lanes.len() - 1
    }

    /// Remove a lane, and give what its cards held.
    pub(super) fn remove_lane(&mut self, lane: usize) -> Option<Vec<CardData>> {
        if lane >= self.lanes.len() {
            return None;
        }
        let cards = self.lanes.remove(lane);
        Some(cards.into_iter().map(|e| e.data).collect())
    }
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
}
