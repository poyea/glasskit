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

const KINDS: &[&str] = &["plain", "countdown"];

fn prompt(kind: &str) -> Option<Prompt> {
    Some(match kind {
        "plain" => Prompt::new("Which planet is the largest?", ["Mars", "Jupiter", "Venus"]),
        "countdown" => Prompt::new("Take a short break?", ["Yes", "Later"])
            .dismiss_label("Skip")
            .auto_dismiss(Some(Duration::from_secs(90))),
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
    let mut popup = ChoicePopup::new(&ui, Placement::default())?;
    println!("{:?}", popup.ask(&prompt)?.response);
    Ok(())
}
