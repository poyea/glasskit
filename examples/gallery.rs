// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Show one kind of popup, by name, until it ends. CI takes a picture of each kind.
//!
//! Run it with `cargo run -p glasskit --example gallery -- <kind>`. With no kind, it lists
//! the kinds.

use std::time::Duration;

use glasskit::Ui;
use glasskit::placement::Placement;
use glasskit::popup::{ChoicePopup, Prompt};

/// The kinds of popup. "stack" shows two popups at once: the second goes above the first.
const KINDS: &[&str] = &["plain", "countdown", "markup", "stack"];

fn prompt(kind: &str) -> Option<Prompt> {
    Some(match kind {
        "plain" => Prompt::new("Which planet is the largest?", ["Mars", "Jupiter", "Venus"]),
        "countdown" => Prompt::new("Take a short break?", ["Yes", "Later"])
            .dismiss_label("Skip")
            .auto_dismiss(Some(Duration::from_secs(90))),
        "stack" => Prompt::new("And a second question?", ["Yes", "No"]),
        "markup" => Prompt::new(
            "What does **`git stash`** do with your *uncommitted* changes?",
            ["Saves them aside", "Deletes them", "Commits them"],
        )
        .markup(),
        _ => return None,
    })
}

fn main() -> glasskit::Result<()> {
    let kind = std::env::args().nth(1).unwrap_or_default();
    let Some(prompt) = prompt(&kind) else {
        println!("kinds: {}", KINDS.join(", "));
        return Ok(());
    };
    let ui = Ui::new()?;
    let mut first = ChoicePopup::new(&ui, Placement::default())?;
    if kind == "stack" {
        first.show(&Prompt::new("A first question?", ["Yes", "No"]))?;
    }
    let mut popup = ChoicePopup::new(&ui, Placement::default())?;
    println!("{:?}", popup.ask(&prompt)?.response);
    Ok(())
}
