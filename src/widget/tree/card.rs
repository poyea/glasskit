// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! The text and look of a card: its title, its body, its pin and its tint.

use super::*;

impl Tree {
    pub(super) fn add_card_text(&mut self, title: String, body: String) -> Vec<usize> {
        let title = self.add(Node::leaf(Content::Text {
            text: title,
            style: TextStyle::BodyStrong,
        }));
        let mut text = Node::leaf(Content::Text {
            text: body,
            style: TextStyle::Body,
        });
        text.visible = !matches!(&text.content, Content::Text { text, .. } if text.is_empty());
        let body = self.add(text);
        vec![title, body]
    }

    pub fn card_text(&self, card: usize) -> Option<(String, String)> {
        match &self.nodes.get(card)?.content {
            Content::Card { text, .. } => text.clone(),
            _ => None,
        }
    }

    /// Change the title and the body of a card that has a text.
    pub fn set_card_text(&mut self, card: usize, title: &str, body: &str) {
        let Some(Content::Card {
            text: Some(text), ..
        }) = self.nodes.get_mut(card).map(|n| &mut n.content)
        else {
            return;
        };
        *text = (title.to_owned(), body.to_owned());
        let &[title_node, body_node, ..] = self.nodes[card].children() else {
            return;
        };
        for (node, value) in [(title_node, title), (body_node, body)] {
            if let Content::Text { text, .. } = &mut self.nodes[node].content {
                *text = value.to_owned();
            }
        }
        self.nodes[body_node].visible = !body.is_empty();
    }

    pub fn set_pinned(&mut self, card: usize, to: bool) {
        if let Some(Content::Card { pinned, .. }) = self.nodes.get_mut(card).map(|n| &mut n.content)
        {
            *pinned = to;
        }
    }

    pub fn set_tint(&mut self, card: usize, to: Option<Tint>) {
        if let Some(Content::Card { tint, .. }) = self.nodes.get_mut(card).map(|n| &mut n.content) {
            *tint = to;
        }
    }
}
