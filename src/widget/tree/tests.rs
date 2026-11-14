// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Tests of the widget tree.

use super::*;

/// Each character is 8 DIPs wide and a line is 20 DIPs high. Text wraps by character.
struct Fixed;

impl Measure for Fixed {
    fn text(&self, text: &str, _: TextStyle, max_width: f32) -> (f32, f32) {
        let chars = text.chars().count() as f32;
        let per_line = (max_width / 8.0).floor().max(1.0);
        let lines = (chars / per_line).ceil().max(1.0);
        ((chars.min(per_line)) * 8.0, lines * 20.0)
    }

    fn label(&self, label: &str) -> (f32, f32) {
        (label.chars().count() as f32 * 8.0, 20.0)
    }
}

fn text(s: &str) -> Node {
    Node::leaf(Content::Text {
        text: s.into(),
        style: TextStyle::Body,
    })
}

fn button(s: &str) -> Node {
    Node::leaf(Content::Button {
        label: s.into(),
        accent: false,
        icon: false,
    })
}

fn with(mut node: Node, children: &[usize]) -> Node {
    node.container.as_mut().unwrap().children = children.to_vec();
    node
}

fn scroll(children: &[usize]) -> Node {
    let mut node = with(
        Node::container(
            Content::Scroll {
                offset: 0.0,
                extent: 0.0,
            },
            Axis::Column,
            Align::Stretch,
        ),
        children,
    );
    node.height = Length::Fill(1.0);
    node
}

/// Five buttons of 32 DIPs in a scroll of 100 DIPs: 160 DIPs, so 60 do not show.
fn five_in_a_scroll() -> (Tree, usize, Vec<usize>) {
    let mut t = Tree::default();
    let buttons: Vec<usize> = (0..5).map(|i| t.add(button(&format!("B{i}")))).collect();
    let s = t.add(scroll(&buttons));
    t.root = Some(s);
    t.layout(200.0, 100.0, &Fixed);
    (t, s, buttons)
}

#[test]
fn a_scroll_moves_its_children_and_stays_in_range() {
    let (mut t, s, b) = five_in_a_scroll();
    assert_eq!(t.nodes[b[4]].rect, Rect::new(0.0, 128.0, 200.0, 32.0));
    assert_eq!(
        t.nodes[s].content,
        Content::Scroll {
            offset: 0.0,
            extent: 160.0
        }
    );

    t.scroll_by(s, 40.0);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[b[0]].rect.y, -40.0);
    assert_eq!(t.nodes[b[4]].rect.y, 88.0);

    t.scroll_by(s, 500.0);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[b[4]].rect.y, 68.0);
    t.scroll_by(s, -500.0);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[b[0]].rect.y, 0.0);
}

#[test]
fn a_scroll_cuts_what_is_outside_it() {
    let (t, s, b) = five_in_a_scroll();
    assert_eq!(t.clip(b[3]), Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
    assert_eq!(t.clip(s), None);
    // The fourth button is at 96 to 128: only its top shows, and only there it is hit.
    assert_eq!(t.hit(5.0, 98.0), Some(b[3]));
    assert_eq!(t.hit(5.0, 110.0), None);
}

#[test]
fn the_wheel_finds_a_scroll_that_can_move() {
    let (mut t, s, _) = five_in_a_scroll();
    assert_eq!(t.scroll_at(5.0, 50.0, 10.0), Some(s));
    assert_eq!(t.scroll_at(5.0, 50.0, -10.0), None);
    assert_eq!(t.scroll_at(5.0, 150.0, 10.0), None);
    t.scroll_by(s, 60.0);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.scroll_at(5.0, 50.0, 10.0), None);
    assert_eq!(t.scroll_at(5.0, 50.0, -10.0), Some(s));
}

#[test]
fn reveal_scrolls_a_widget_into_view() {
    let (mut t, s, b) = five_in_a_scroll();
    assert!(t.reveal(b[4]));
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[b[4]].rect.y + 32.0, 100.0);
    assert!(!t.reveal(b[4]));
    assert!(t.reveal(b[0]));
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[b[0]].rect.y, 0.0);
    assert!(!t.reveal(s));
}

#[test]
fn a_short_scroll_does_not_move() {
    let mut t = Tree::default();
    let ok = t.add(button("OK"));
    let s = t.add(scroll(&[ok]));
    t.root = Some(s);
    t.layout(200.0, 100.0, &Fixed);
    t.scroll_by(s, 50.0);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[ok].rect.y, 0.0);
    assert_eq!(t.scroll_at(5.0, 5.0, 10.0), None);
}

#[test]
fn a_hidden_widget_takes_no_space_and_no_gap() {
    let mut t = Tree::default();
    let a = t.add(text("one"));
    let b = t.add(button("OK"));
    let c = t.add(text("two"));
    let mut col = with(
        Node::container(Content::Blank, Axis::Column, Align::Stretch),
        &[a, b, c],
    );
    col.container.as_mut().unwrap().gap = 10.0;
    let col = t.add(col);
    t.root = Some(col);
    t.nodes[b].visible = false;
    assert_eq!(t.measure(col, 200.0, 100.0, &Fixed), (24.0, 50.0));
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[c].rect, Rect::new(0.0, 30.0, 200.0, 20.0));
    assert_eq!(t.draw_order(), [col, a, c]);
    assert!(!t.focusable(b));
    assert_eq!(t.hit(5.0, 25.0), None);

    t.nodes[b].visible = true;
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[c].rect, Rect::new(0.0, 72.0, 200.0, 20.0));
    assert_eq!(t.hit(5.0, 35.0), Some(b));
}

#[test]
fn a_hidden_container_hides_its_widgets() {
    let mut t = Tree::default();
    let ok = t.add(button("OK"));
    let page = t.add(with(
        Node::container(Content::Blank, Axis::Column, Align::Stretch),
        &[ok],
    ));
    let root = t.add(with(
        Node::container(Content::Blank, Axis::Stack, Align::Stretch),
        &[page],
    ));
    t.root = Some(root);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.hit(5.0, 5.0), Some(ok));
    t.nodes[page].visible = false;
    assert_eq!(t.hit(5.0, 5.0), None);
    assert!(!t.focusable(ok));
    assert_eq!(t.next_focus(None, false), None);
}

#[test]
fn a_navigation_item_has_space_for_its_mark() {
    let mut t = Tree::default();
    let item = t.add(nav("Inbox", None));
    assert_eq!(t.measure(item, 200.0, 100.0, &Fixed), (68.0, 36.0));
    assert!(t.nodes[item].content.interactive());
    let with_icon = t.add(nav("Inbox", Some("i")));
    assert_eq!(t.measure(with_icon, 200.0, 100.0, &Fixed), (100.0, 36.0));
}

fn nav(label: &str, icon: Option<&str>) -> Node {
    Node::leaf(Content::NavItem {
        label: label.into(),
        icon: icon.map(Into::into),
        selected: false,
    })
}

/// A side bar of 200 DIPs with two items, at the left of a page, in a window of 400 by
/// 300 DIPs.
fn side_bar(modes: &[SideMode], title: &str) -> (Tree, usize, [usize; 2], usize) {
    let mut t = Tree::default();
    let items = [t.add(nav("Home", Some("h"))), t.add(nav("Mail", None))];
    let bar = t.add(with(
        Node::container(
            Content::SideBar {
                mode: SideMode::Expanded,
                modes: modes.to_vec(),
                width: 200.0,
                title: title.into(),
            },
            Axis::Column,
            Align::Stretch,
        ),
        &items,
    ));
    let mut page = text("page");
    page.width = Length::Fill(1.0);
    let page = t.add(page);
    let row = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Stretch),
        &[bar, page],
    ));
    t.root = Some(row);
    t.layout(400.0, 300.0, &Fixed);
    (t, bar, items, page)
}

fn toggle_of(t: &Tree, bar: usize) -> usize {
    let head = t.nodes[bar].children()[0];
    t.nodes[head].children()[0]
}

fn card(title: &str, body: &str) -> Node {
    let mut node = Node::container(
        Content::Card {
            text: Some((title.into(), body.into())),
            pinned: false,
            tint: None,
        },
        Axis::Stack,
        Align::Stretch,
    );
    node.clickable = true;
    node.draggable = true;
    node
}

/// Two lanes of 100 DIPs side by side, each a column with a gap of 10 DIPs. The first
/// lane has two cards, and the second lane has one.
fn lanes() -> (Tree, [usize; 2], [usize; 3]) {
    let mut t = Tree::default();
    let cards = [
        t.add(card("A", "")),
        t.add(card("B", "")),
        t.add(card("C", "")),
    ];
    let mut lanes = [0; 2];
    for (lane, own) in [&cards[..2], &cards[2..]].into_iter().enumerate() {
        let mut node = with(
            Node::container(Content::Blank, Axis::Column, Align::Stretch),
            own,
        );
        node.container.as_mut().unwrap().gap = 10.0;
        node.width = Length::Fixed(100.0);
        node.drop_target = true;
        lanes[lane] = t.add(node);
    }
    let row = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Stretch),
        &lanes,
    ));
    t.root = Some(row);
    t.layout(200.0, 300.0, &Fixed);
    (t, lanes, cards)
}

#[test]
fn a_card_with_a_text_stacks_its_title_and_its_body() {
    let mut t = Tree::default();
    let extra = t.add(button("Open"));
    let c = t.add(with(card("Plan", "Write it"), &[extra]));
    t.root = Some(c);
    t.layout(200.0, 200.0, &Fixed);
    let [title, body, last] = t.nodes[c].children() else {
        panic!("a card with a text has its texts before its children");
    };
    assert_eq!(*last, extra);
    assert_eq!(t.nodes[*title].rect, Rect::new(0.0, 0.0, 200.0, 20.0));
    assert_eq!(t.nodes[*body].rect, Rect::new(0.0, 24.0, 200.0, 20.0));
    assert_eq!(t.card_text(c), Some(("Plan".into(), "Write it".into())));

    t.set_card_text(c, "Plan B", "");
    t.layout(200.0, 200.0, &Fixed);
    assert_eq!(t.card_text(c), Some(("Plan B".into(), String::new())));
    assert_eq!(t.nodes[extra].rect.y, 24.0);
    assert_eq!(t.focus_order(), [c, extra]);
    assert_eq!(t.hit(5.0, 5.0), Some(c));
    assert_eq!(t.hit(5.0, 30.0), Some(extra));
}

#[test]
fn a_drag_starts_on_a_card_and_drops_between_cards() {
    let (t, lanes, cards) = lanes();
    // Each card is 20 DIPs high: A at 0, B at 30.
    assert_eq!(t.drag_source_at(50.0, 35.0), Some(cards[1]));
    assert_eq!(t.drag_source_at(50.0, 100.0), None);

    let place = t.drop_at(150.0, 5.0, cards[0]).unwrap();
    assert_eq!(
        (place.target, place.index, place.changes),
        (lanes[1], 0, true)
    );
    assert_eq!(place.line, Some(Rect::new(100.0, -1.0, 100.0, 2.0)));
    let place = t.drop_at(150.0, 200.0, cards[0]).unwrap();
    assert_eq!((place.index, place.changes), (1, true));
    assert_eq!(place.line, Some(Rect::new(100.0, 24.0, 100.0, 2.0)));

    // In its own lane, A can go after B, and a drop at its own place changes nothing.
    let place = t.drop_at(50.0, 200.0, cards[0]).unwrap();
    assert_eq!(
        (place.target, place.index, place.changes),
        (lanes[0], 1, true)
    );
    let place = t.drop_at(50.0, 5.0, cards[0]).unwrap();
    assert_eq!((place.index, place.changes, place.line), (0, false, None));
    assert_eq!(t.drop_at(250.0, 5.0, cards[0]), None);
}

#[test]
fn new_children_replace_the_old_ones() {
    let (mut t, lanes, cards) = lanes();
    t.set_children(lanes[1], &[cards[0], cards[2], cards[0], lanes[1]]);
    assert_eq!(t.nodes[lanes[0]].children(), [cards[1]]);
    assert_eq!(t.nodes[lanes[1]].children(), [cards[0], cards[2]]);
    t.set_children(lanes[1], &[]);
    assert_eq!(t.nodes[cards[2]].parent, None);
    assert!(!t.shown(cards[2]));

    // The texts of a card stay when its children change.
    let extra = t.add(button("Open"));
    t.set_children(cards[1], &[extra]);
    assert_eq!(t.nodes[cards[1]].children().len(), 3);
    assert_eq!(t.nodes[cards[1]].children()[2], extra);
}

#[test]
fn a_side_bar_has_a_width_for_each_mode() {
    let (mut t, bar, items, page) = side_bar(&SideMode::ALL, "App");
    let toggle = toggle_of(&t, bar);
    assert_eq!(t.nodes[bar].rect.w, 200.0);
    assert_eq!(t.nodes[toggle].rect, Rect::new(0.0, 0.0, 48.0, 36.0));
    assert_eq!(t.nodes[items[0]].rect, Rect::new(0.0, 36.0, 200.0, 36.0));
    assert_eq!(t.nodes[page].rect.x, 200.0);

    t.set_side_mode(bar, SideMode::Icons);
    t.layout(400.0, 300.0, &Fixed);
    assert_eq!(t.nodes[items[1]].rect, Rect::new(0.0, 72.0, 48.0, 36.0));
    assert_eq!(t.nodes[page].rect.x, 48.0);
    assert_eq!(t.focus_order(), [toggle, items[0], items[1]]);

    t.set_side_mode(bar, SideMode::Collapsed);
    t.layout(400.0, 300.0, &Fixed);
    assert_eq!(t.nodes[bar].rect.w, 48.0);
    assert_eq!(t.focus_order(), [toggle]);
    assert_eq!(t.hit(5.0, 40.0), None);
    assert!(!t.shown(items[0]));
}

#[test]
fn the_button_of_a_side_bar_steps_through_its_modes() {
    let (mut t, bar, _, _) = side_bar(&[SideMode::Expanded, SideMode::Collapsed], "");
    let toggle = toggle_of(&t, bar);
    assert_eq!(t.step_side(toggle), Some(bar));
    assert_eq!(t.side_mode(bar), Some(SideMode::Collapsed));
    t.step_side(toggle);
    assert_eq!(t.side_mode(bar), Some(SideMode::Expanded));
    // From a mode that is not in the list, the button goes to the first one.
    t.set_side_mode(bar, SideMode::Icons);
    t.step_side(toggle);
    assert_eq!(t.side_mode(bar), Some(SideMode::Expanded));
}

#[test]
fn a_side_bar_with_one_mode_has_no_button() {
    let (t, _, items, _) = side_bar(&[SideMode::Expanded], "");
    assert_eq!(t.focus_order(), items);
    assert_eq!(t.nodes[items[0]].rect.y, 0.0);
    let (t, bar, items, _) = side_bar(&[], "App");
    let head = t.nodes[bar].children()[0];
    assert!(t.shown(head));
    assert_eq!(t.focus_order(), items);
}

#[test]
fn a_column_stacks_and_stretches() {
    let mut t = Tree::default();
    let a = t.add(text("hello"));
    let b = t.add(button("OK"));
    let col = t.add(with(
        Node::container(Content::Blank, Axis::Column, Align::Stretch),
        &[a, b],
    ));
    t.root = Some(col);
    t.layout(200.0, 100.0, &Fixed);
    assert_eq!(t.nodes[a].rect, Rect::new(0.0, 0.0, 200.0, 20.0));
    assert_eq!(t.nodes[b].rect, Rect::new(0.0, 20.0, 200.0, 32.0));
}

#[test]
fn fills_share_what_is_left_by_weight() {
    let mut t = Tree::default();
    let ok = t.add(button("OK"));
    let mut one = Node::leaf(Content::Blank);
    one.width = Length::Fill(1.0);
    let one = t.add(one);
    let mut two = Node::leaf(Content::Blank);
    two.width = Length::Fill(2.0);
    let two = t.add(two);
    let mut row = with(
        Node::container(Content::Blank, Axis::Row, Align::Center),
        &[one, ok, two],
    );
    row.container.as_mut().unwrap().gap = 8.0;
    let row = t.add(row);
    t.root = Some(row);
    t.layout(400.0, 100.0, &Fixed);
    // 400 - 64 (button) - 16 (gaps) = 320, split 1:2.
    assert!((t.nodes[one].rect.w - 106.666_67).abs() < 0.01);
    assert_eq!(t.nodes[ok].rect.x, t.nodes[one].rect.w + 8.0);
    assert!((t.nodes[two].rect.w - 213.333_33).abs() < 0.01);
    // Centered across.
    assert_eq!(t.nodes[ok].rect.y, 34.0);
}

#[test]
fn text_wraps_to_the_column_width() {
    let mut t = Tree::default();
    let long = t.add(text("abcdefghij"));
    let col = t.add(with(
        Node::container(Content::Blank, Axis::Column, Align::Stretch),
        &[long],
    ));
    t.root = Some(col);
    t.layout(40.0, 200.0, &Fixed);
    assert_eq!(t.nodes[long].rect.h, 40.0);
}

#[test]
fn padding_and_gap_are_inside_and_between() {
    let mut t = Tree::default();
    let a = t.add(text("a"));
    let b = t.add(text("b"));
    let mut col = with(
        Node::container(Content::card(), Axis::Column, Align::Start),
        &[a, b],
    );
    let c = col.container.as_mut().unwrap();
    c.padding = Insets::all(10.0);
    c.gap = 5.0;
    let col = t.add(col);
    t.root = Some(col);
    t.layout(100.0, 100.0, &Fixed);
    assert_eq!(t.nodes[a].rect, Rect::new(10.0, 10.0, 8.0, 20.0));
    assert_eq!(t.nodes[b].rect, Rect::new(10.0, 35.0, 8.0, 20.0));
    assert_eq!(t.measure(col, 100.0, 100.0, &Fixed), (28.0, 65.0));
}

#[test]
fn tab_goes_through_enabled_buttons_and_wraps() {
    let mut t = Tree::default();
    let a = t.add(button("A"));
    let mut off = button("off");
    off.enabled = false;
    let off = t.add(off);
    let b = t.add(button("B"));
    let row = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Center),
        &[a, off, b],
    ));
    t.root = Some(row);
    assert_eq!(t.focus_order(), [a, b]);
    assert_eq!(t.next_focus(None, false), Some(a));
    assert_eq!(t.next_focus(Some(a), false), Some(b));
    assert_eq!(t.next_focus(Some(b), false), Some(a));
    assert_eq!(t.next_focus(Some(a), true), Some(b));
}

#[test]
fn an_icon_button_is_a_square() {
    let mut t = Tree::default();
    let icon = t.add(Node::leaf(Content::Button {
        label: "a long label".into(),
        accent: false,
        icon: true,
    }));
    let (w, h) = t.measure(icon, 500.0, 500.0, &Fixed);
    assert_eq!(w, h);
    assert_eq!(w, BUTTON_MIN.1);
}

#[test]
fn a_click_goes_to_the_button_under_it() {
    let mut t = Tree::default();
    let a = t.add(button("A"));
    let row = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Start),
        &[a],
    ));
    t.root = Some(row);
    t.layout(300.0, 100.0, &Fixed);
    assert_eq!(t.hit(10.0, 10.0), Some(a));
    assert_eq!(t.hit(200.0, 10.0), None);
}

#[test]
fn a_child_moves_to_its_new_container() {
    let mut t = Tree::default();
    let a = t.add(text("a"));
    let first = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Start),
        &[a],
    ));
    let second = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Start),
        &[a],
    ));
    assert!(t.nodes[first].children().is_empty());
    assert_eq!(t.nodes[second].children(), [a]);
    assert_eq!(t.nodes[a].parent, Some(second));
}

#[test]
fn a_child_listed_twice_is_in_its_container_once() {
    let mut t = Tree::default();
    let a = t.add(text("a"));
    let b = t.add(text("b"));
    let col = t.add(with(
        Node::container(Content::Blank, Axis::Column, Align::Start),
        &[a, b, a],
    ));
    assert_eq!(t.nodes[col].children(), [a, b]);
}

fn text_box(text: &str, multiline: bool) -> Node {
    let mut edit = Editor::new(multiline, usize::MAX);
    edit.set_text(text);
    Node::leaf(Content::TextBox {
        edit,
        placeholder: String::new(),
        scroll: 0.0,
    })
}

#[test]
fn a_text_box_takes_focus_and_grows_with_many_lines() {
    let mut t = Tree::default();
    let one = t.add(text_box("abcdefghij", false));
    let many = t.add(text_box("abcdefghij", true));
    let col = t.add(with(
        Node::container(Content::Blank, Axis::Column, Align::Start),
        &[one, many],
    ));
    t.root = Some(col);
    t.layout(60.0, 300.0, &Fixed);
    assert_eq!(t.focus_order(), [one, many]);
    // One line: 20 + 2 * 6 padding, at least 32.
    assert_eq!(t.nodes[one].rect, Rect::new(0.0, 0.0, 60.0, 32.0));
    // 60 - 2 * 11 = 38 DIPs for text: 4 characters a line, so 3 lines.
    assert_eq!(t.nodes[many].rect.h, 72.0);
}

#[test]
fn a_widget_outside_the_root_takes_no_focus() {
    let mut t = Tree::default();
    let a = t.add(button("A"));
    let b = t.add(button("B"));
    let row = t.add(with(
        Node::container(Content::Blank, Axis::Row, Align::Start),
        &[a],
    ));
    t.root = Some(row);
    assert_eq!(t.focus_order(), [a]);
    assert!(!t.focusable(b));
}
