// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A glass window with widgets: a title, a card with a counter, and two buttons.
//!
//! Run it with `cargo run -p glasskit --example window`. Pass `glass` or `solid` to see the
//! other materials.

use glasskit::Ui;
use glasskit::events::{Event, EventLoop};
use glasskit::widget::{Button, Card, Column, Insets, Row, Spacer, Text, TextStyle};
use glasskit::window::{Material, Window, WindowEvent};

fn main() -> glasskit::Result<()> {
    let ui = Ui::new()?;
    let mut events: EventLoop<()> = EventLoop::new(&ui)?;
    let mut window = Window::new(&ui, "glasskit", 420.0, 280.0)?;
    window.set_min_size(320.0, 240.0);
    match std::env::args().nth(1).as_deref() {
        Some("glass") => window.set_material(Material::Glass),
        Some("solid") => window.set_material(Material::Solid),
        _ => {}
    }

    let title = window.add(Text::new("Settings").style(TextStyle::Title));
    let note = window.add(Text::new(
        "Widgets lay out in rows and columns, and the window follows the display scale.",
    ));
    let count = window.add(Text::new("Count: 0").style(TextStyle::BodyStrong));
    let add = window.add(Button::new("Add one"));
    let counter = window.add(Row::new().gap(12.0).child(count).child(add));
    let card = window.add(Card::new().child(counter));
    let spacer = window.add(Spacer::fill());
    let cancel = window.add(Button::new("Cancel"));
    let ok = window.add(Button::new("OK").accent());
    let buttons = window.add(Row::new().gap(8.0).child(spacer).child(cancel).child(ok));
    let gap = window.add(Spacer::fill());
    let root = window.add(
        Column::new()
            .padding(Insets::all(24.0))
            .gap(12.0)
            .child(title)
            .child(note)
            .child(card)
            .child(gap)
            .child(buttons),
    );
    window.set_root(root);
    window.show();

    let mut n = 0;
    loop {
        match events.next(None)? {
            Event::Window(id) if id == window.id() => {
                while let Some(event) = window.next_event() {
                    match event {
                        WindowEvent::Clicked(b) if b == add => {
                            n += 1;
                            window.set_text(count, &format!("Count: {n}"));
                            println!("count {n}");
                        }
                        WindowEvent::Clicked(b) if b == ok => {
                            println!("ok");
                            return Ok(());
                        }
                        WindowEvent::Clicked(b) if b == cancel => return Ok(()),
                        WindowEvent::CloseRequested => return Ok(()),
                        _ => {}
                    }
                }
            }
            Event::Quit => return Ok(()),
            _ => {}
        }
    }
}
