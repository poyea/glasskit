// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A tray app that asks a question each minute and on "Ask now".
//!
//! Run it with `cargo run -p glasskit --example quiz`.

use std::time::{Duration, SystemTime};

use glasskit::Ui;
use glasskit::events::{Event, EventLoop};
use glasskit::placement::Placement;
use glasskit::popup::{ChoicePopup, Mark, Prompt};
use glasskit::tray::{Icon, MenuItem, Tray};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    AskNow,
}

fn main() -> glasskit::Result<()> {
    let ui = Ui::new()?;
    let mut events = EventLoop::new(&ui)?;
    let size = Icon::tray_size();
    let icon = Icon::from_argb(size, vec![0xFF2E_86DE; (size * size) as usize])?;
    let mut tray = Tray::new(&mut events, "Quiz", &icon)?;
    tray.set_menu(&[
        MenuItem::command(Command::AskNow, "Ask now"),
        MenuItem::Separator,
        MenuItem::Quit("Quit".into()),
    ]);
    let mut popup = ChoicePopup::new(&ui, Placement::default())?;
    let mut next = SystemTime::now() + Duration::from_secs(60);
    loop {
        match events.next(Some(next))? {
            Event::Quit => return Ok(()),
            Event::Deadline | Event::Command(Command::AskNow) => {
                next = SystemTime::now() + Duration::from_secs(60);
                if !popup.is_showing() {
                    let prompt = Prompt::new("What is 2 + 2?", ["3", "4", "5"])
                        .marks(|clicked| answer_marks(clicked, 1))
                        .auto_dismiss(Some(Duration::from_secs(30)));
                    popup.show(&prompt)?;
                }
            }
            Event::Popup(id) if id == popup.id() => {
                if let Some(reply) = popup.take_reply() {
                    println!("{:?}", reply.response);
                }
            }
            _ => {}
        }
    }
}

/// Mark the correct answer good and a wrong click bad.
fn answer_marks(clicked: usize, correct: usize) -> Vec<(usize, Mark)> {
    let mut marks = vec![(correct, Mark::Good)];
    if clicked != correct {
        marks.push((clicked, Mark::Bad));
    }
    marks
}
