// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Show one kind of popup, by name, until it ends. CI takes a picture of each kind.
//!
//! Run it with `cargo run -p glasskit --example gallery -- <kind>`. With no kind, it lists
//! the kinds.

use std::time::Duration;

use glasskit::Ui;
use glasskit::placement::Placement;
use glasskit::popup::{ChoicePopup, Image, Prompt};

/// The kinds of popup. "stack" shows two popups at once: the second goes above the first.
const KINDS: &[&str] = &["plain", "countdown", "markup", "stack", "image", "input"];

fn prompt(kind: &str) -> Option<Prompt> {
    Some(match kind {
        "plain" => Prompt::new("Which planet is the largest?", ["Mars", "Jupiter", "Venus"]),
        "countdown" => Prompt::new("Take a short break?", ["Yes", "Later"])
            .dismiss_label("Skip")
            .auto_dismiss(Some(Duration::from_secs(90))),
        "input" => Prompt::new("How do you write 'remember' in Chinese?", [] as [&str; 0])
            .input("Type the answer")
            .dismiss_label("Skip"),
        "image" => {
            Prompt::new("Which shape is this?", ["Circle", "Square", "Ring"]).image(ring(160))
        }
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

/// A blue ring on a clear background, `size` pixels square.
fn ring(size: u32) -> Image {
    let center = size as f32 / 2.0;
    let (outer, inner) = (center - 2.0, center * 0.55);
    let pixels: Vec<u32> = (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as f32 + 0.5, (i / size) as f32 + 0.5);
            let d = ((x - center).powi(2) + (y - center).powi(2)).sqrt();
            let alpha = (outer - d + 0.5).clamp(0.0, 1.0) * (d - inner + 0.5).clamp(0.0, 1.0);
            ((alpha * 255.0) as u32) << 24 | 0x002E_86DE
        })
        .collect();
    Image::from_argb(size, size, &pixels).expect("the size matches the pixels")
}
