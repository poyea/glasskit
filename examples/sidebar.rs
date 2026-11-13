// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! A window with a side bar that the user can make narrow.
//!
//! Run it with `cargo run -p glasskit --example sidebar`. Pass `icons` or `collapsed` to
//! start in that mode.

use glasskit::Ui;
use glasskit::events::{Event, EventLoop};
use glasskit::widget::{
    Align, Column, Insets, Length, NavItem, Row, SideBar, SideMode, Spacer, Stack, Text, TextStyle,
};
use glasskit::window::{Window, WindowEvent};

const PAGES: [(&str, &str); 3] = [
    ("Home", "\u{E80F}"),
    ("Mail", "\u{E715}"),
    ("Settings", "\u{E713}"),
];

fn main() -> glasskit::Result<()> {
    let ui = Ui::new()?;
    let mut events: EventLoop<()> = EventLoop::new(&ui)?;
    let mut window = Window::new(&ui, "Side bar", 560.0, 360.0)?;
    window.set_min_size(360.0, 260.0);

    let items = PAGES.map(|(name, icon)| window.add(NavItem::new(name).icon(icon)));
    let pages = PAGES.map(|(name, _)| {
        let title = window.add(Text::new(name).style(TextStyle::Title));
        window.add(Column::new().padding(Insets::all(16.0)).child(title))
    });
    let gap = window.add(Spacer::fill());
    let mode = match std::env::args().nth(1).as_deref() {
        Some("icons") => SideMode::Icons,
        Some("collapsed") => SideMode::Collapsed,
        _ => SideMode::Expanded,
    };
    let side = window.add(
        SideBar::new()
            .title("glasskit")
            .expanded_width(180.0)
            .mode(mode)
            .gap(4.0)
            .child(items[0])
            .child(items[1])
            .child(gap)
            .child(items[2]),
    );
    let stack = window.add(Stack::new().width(Length::Fill(1.0)).children(pages));
    let root = window.add(
        Row::new()
            .padding(Insets::all(8.0))
            .gap(8.0)
            .align(Align::Stretch)
            .child(side)
            .child(stack),
    );
    window.set_root(root);
    let show = |window: &mut Window, shown: usize| {
        for (i, (item, page)) in items.into_iter().zip(pages).enumerate() {
            window.set_selected(item, i == shown);
            window.set_visible(page, i == shown);
        }
    };
    show(&mut window, 0);
    window.show();

    loop {
        match events.next(None)? {
            Event::Window(id) if id == window.id() => {
                while let Some(event) = window.next_event() {
                    match event {
                        WindowEvent::Clicked(id) => {
                            if let Some(shown) = items.iter().position(|&item| id == item) {
                                show(&mut window, shown);
                            }
                        }
                        WindowEvent::SideModeChanged(id) if id == side => {
                            println!("{:?}", window.side_mode(side));
                        }
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
