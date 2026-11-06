// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! An app with only a dialog: a small form placed with rows and columns.
//!
//! Run it with `cargo run -p glasskit --example dialog`.

use glasskit::Ui;
use glasskit::dialog::{Bounds, Dialog, EditStyle, Event, Flow};
use glasskit::layout::{Column, Size};

fn main() -> glasskit::Result<()> {
    let ui = Ui::new()?;
    let (width, height) = (420, 220);
    let mut d = Dialog::new(&ui, "Profile", width, height)?;
    let mut rows = Column::new(Bounds::new(0, 0, width, height).inset(16), 10);

    let field = |row: Bounds| row.columns([Size::Fixed(110), Size::Fill(1)], 8);
    let [label, control] = field(rows.next(26));
    d.add_label("Name", label)?;
    let name = d.add_edit(EditStyle::default(), control)?;
    let [label, control] = field(rows.next(26));
    d.add_label("Language", label)?;
    let language = d.add_drop_down(control)?;
    d.set_items(language, &["English", "Cantonese", "Japanese"]);
    d.set_selected(language, Some(0));
    let news = d.add_check("Send me news", rows.next(26))?;

    let [_, save, cancel] = rows
        .next(30)
        .columns([Size::Fill(1), Size::Fixed(96), Size::Fixed(96)], 8);
    let save = d.add_button("Save", save)?;
    let cancel = d.add_button("Cancel", cancel)?;

    d.run(|d, event| match event {
        Event::Clicked(id) if id == save => {
            println!(
                "{} / {:?} / news: {}",
                d.text(name),
                d.selected(language),
                d.checked(news)
            );
            Flow::Close
        }
        Event::Submit => Flow::Continue,
        Event::Clicked(id) if id == cancel => Flow::Close,
        Event::Close => Flow::Close,
        _ => Flow::Continue,
    })?;
    Ok(())
}
