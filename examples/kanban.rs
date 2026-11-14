// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A kanban board: drag a card to another place or lane, and click a card to change it, or
//! to clone, pin, archive, or delete it.
//!
//! Run it with `cargo run -p glasskit --example kanban`. It prints what the user does.

use glasskit::Ui;
use glasskit::events::{Event, EventLoop};
use glasskit::kanban::{Board, BoardEvent, CardData, CardKey};
use glasskit::widget::{Button, Column, Insets, Row, Spacer, Text, TextStyle};
use glasskit::window::{Window, WindowEvent};

const CARDS: [(usize, &str, &str); 5] = [
    (0, "Write the plan", "One page, with the dates."),
    (0, "Book the room", ""),
    (1, "Draft the slides", "Ten slides for the first review."),
    (2, "Pick a date", ""),
    (2, "Send the invitations", "To all the people of the team."),
];

fn main() -> glasskit::Result<()> {
    let ui = Ui::new()?;
    let mut events: EventLoop<()> = EventLoop::new(&ui)?;
    let mut window = Window::new(&ui, "Kanban", 820.0, 520.0)?;
    window.set_min_size(560.0, 360.0);

    let mut board = Board::new(&mut window, &["To do", "Doing", "Done"]);
    for (lane, title, body) in CARDS {
        board.add_card(&mut window, lane, CardData::new(title, body));
    }
    let pinned = CardData {
        pinned: true,
        ..CardData::new("Ask for the budget", "")
    };
    board.add_card(&mut window, 1, pinned);

    let title = window.add(Text::new("Board").style(TextStyle::Title));
    let gap = window.add(Spacer::fill());
    let archived = window.add(Button::new("Show archived"));
    let top = window.add(Row::new().child(title).child(gap).child(archived));
    let root = window.add(
        Column::new()
            .padding(Insets::all(16.0))
            .gap(12.0)
            .child(top)
            .child(board.widget()),
    );
    window.set_root(root);
    window.show();

    loop {
        match events.next(None)? {
            Event::Window(id) if id == window.id() => {
                while let Some(event) = window.next_event() {
                    match event {
                        WindowEvent::CloseRequested => return Ok(()),
                        WindowEvent::Clicked(id) if id == archived => {
                            let show = !board.show_archived();
                            board.set_show_archived(&mut window, show);
                            let label = if show {
                                "Hide archived"
                            } else {
                                "Show archived"
                            };
                            window.set_text(archived, label);
                        }
                        event => {
                            if let Some(change) = board.handle(&ui, &mut window, &event)? {
                                println!("{}", describe(&board, &change));
                            }
                        }
                    }
                }
            }
            Event::Quit => return Ok(()),
            _ => {}
        }
    }
}

fn describe(board: &Board, change: &BoardEvent) -> String {
    let title = |key: &CardKey| board.card(*key).map_or("", |card| card.title.as_str());
    let lane = |key: &CardKey| {
        let lane = board.place(*key).map_or(0, |(lane, _)| lane);
        board.lane_name(lane).unwrap_or("")
    };
    match change {
        BoardEvent::Added(key) => format!("added {} to {}", title(key), lane(key)),
        BoardEvent::Edited(key) => format!("edited {}", title(key)),
        BoardEvent::Moved(key) => {
            let at = board.place(*key).map_or(0, |(_, at)| at);
            format!("moved {} to {} at {at}", title(key), lane(key))
        }
        BoardEvent::Cloned { from, .. } => format!("cloned {}", title(from)),
        BoardEvent::Pinned(key) => format!("pinned {}", title(key)),
        BoardEvent::Archived(key) => format!("archived {}", title(key)),
        BoardEvent::Deleted { card, .. } => format!("deleted {}", card.title),
        _ => String::new(),
    }
}
