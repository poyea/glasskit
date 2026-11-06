// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Where a window shows in an area, as fractions of the free space in that area.

/// The space between a window and the edge of the work area, at 100% display scale.
pub(crate) const SCREEN_MARGIN: f32 = 12.0;

/// `x` and `y` go from 0 (the left or top edge) to 1 (the right or bottom edge) of the free
/// space, that is, the area less the window size and a margin on each side. Thus 0.5 is the
/// center, and the same placement gives the same relative position at all resolutions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    x: f32,
    y: f32,
}

impl Default for Placement {
    fn default() -> Self {
        Self::new(1.0, 1.0)
    }
}

impl Placement {
    /// Values out of 0..=1 are clamped. A value that is not a number becomes 0.5.
    pub fn new(x: f32, y: f32) -> Self {
        let fraction = |v: f32| if v.is_nan() { 0.5 } else { v.clamp(0.0, 1.0) };
        Self {
            x: fraction(x),
            y: fraction(y),
        }
    }

    /// Where the window is across the free space: 0 is the left edge and 1 the right edge.
    pub fn x(self) -> f32 {
        self.x
    }

    /// Where the window is down the free space: 0 is the top edge and 1 the bottom edge.
    pub fn y(self) -> f32 {
        self.y
    }

    /// The top-left corner of a window of this size in `area`, in whole pixels.
    /// If the window is larger than the area, its top-left corner stays in the area, so the
    /// start of the content is visible.
    pub(crate) fn place(self, area: Area, width: i32, height: i32, margin: i32) -> (i32, i32) {
        let (x, y) = self.place_exact(area, width as f32, height as f32, margin as f32);
        (x.round() as i32, y.round() as i32)
    }

    /// [`Placement::place`] without rounding, for a drawing of the area at a smaller scale.
    pub(crate) fn place_exact(
        self,
        area: Area,
        width: f32,
        height: f32,
        margin: f32,
    ) -> (f32, f32) {
        (
            along(area.left, area.right, width, margin, self.x),
            along(area.top, area.bottom, height, margin, self.y),
        )
    }

    /// The placement that puts the top-left corner of a window of this size at `(x, y)`.
    pub(crate) fn at_exact(
        area: Area,
        width: f32,
        height: f32,
        margin: f32,
        (x, y): (f32, f32),
    ) -> Self {
        Self::new(
            fraction_at(area.left, area.right, width, margin, x),
            fraction_at(area.top, area.bottom, height, margin, y),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl From<(i32, i32, i32, i32)> for Area {
    fn from((left, top, right, bottom): (i32, i32, i32, i32)) -> Self {
        Self {
            left: left as f32,
            top: top as f32,
            right: right as f32,
            bottom: bottom as f32,
        }
    }
}

/// A window rectangle on the screen, in whole pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}

/// Move `rect` up (or down) past the windows in `others` that it covers, with `gap` pixels
/// between them. If the result does not fit between `top` and `bottom`, it gives `rect`
/// as it was: a window that covers another is better than a window off the screen.
pub(crate) fn stack(
    rect: Rect,
    others: &[Rect],
    up: bool,
    gap: i32,
    top: i32,
    bottom: i32,
) -> Rect {
    let mut moved = rect;
    // Each step moves past one more window, so there are at most `others.len()` steps.
    for _ in 0..others.len() {
        let Some(hit) = others.iter().find(|other| moved.overlaps(other)) else {
            break;
        };
        moved.y = if up {
            hit.y - gap - moved.height
        } else {
            hit.y + hit.height + gap
        };
    }
    let fits = moved.y >= top && moved.y + moved.height <= bottom;
    if fits && !others.iter().any(|other| moved.overlaps(other)) {
        moved
    } else {
        rect
    }
}

fn along(start: f32, end: f32, size: f32, margin: f32, fraction: f32) -> f32 {
    let free = end - start - 2.0 * margin - size;
    (start + margin + fraction * free).max(start)
}

fn fraction_at(start: f32, end: f32, size: f32, margin: f32, position: f32) -> f32 {
    let free = end - start - 2.0 * margin - size;
    if free <= 0.0 {
        return 0.5;
    }
    (position - start - margin) / free
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(y: i32) -> Rect {
        Rect {
            x: 600,
            y,
            width: 360,
            height: 200,
        }
    }

    #[test]
    fn a_free_place_stays() {
        assert_eq!(stack(rect(500), &[rect(100)], true, 8, 0, 800), rect(500));
    }

    #[test]
    fn a_popup_goes_above_the_ones_that_show() {
        let others = [rect(588), rect(380)];
        assert_eq!(stack(rect(588), &others, true, 8, 0, 800), rect(172));
    }

    #[test]
    fn a_popup_goes_below_from_the_top() {
        assert_eq!(stack(rect(12), &[rect(12)], false, 8, 0, 800), rect(220));
    }

    #[test]
    fn a_full_stack_keeps_the_first_place() {
        let others = [rect(588), rect(380), rect(172)];
        assert_eq!(stack(rect(588), &others, true, 8, 0, 800), rect(588));
    }

    const AREA: Area = Area {
        left: 0.0,
        top: 0.0,
        right: 1000.0,
        bottom: 800.0,
    };

    #[test]
    fn places_corners_and_center() {
        assert_eq!(Placement::new(0.0, 0.0).place(AREA, 100, 50, 10), (10, 10));
        assert_eq!(
            Placement::new(1.0, 1.0).place(AREA, 100, 50, 10),
            (890, 740)
        );
        assert_eq!(
            Placement::new(0.5, 0.5).place(AREA, 100, 50, 10),
            (450, 375)
        );
        assert_eq!(
            Placement::new(0.5, 1.0).place(AREA, 100, 50, 10),
            (450, 740)
        );
    }

    #[test]
    fn places_between_the_edges() {
        assert_eq!(
            Placement::new(0.25, 0.75).place(AREA, 100, 50, 10),
            (230, 558)
        );
    }

    #[test]
    fn window_taller_than_area_keeps_its_top_visible() {
        assert_eq!(Placement::new(1.0, 1.0).place(AREA, 100, 900, 10), (890, 0));
        assert_eq!(Placement::new(0.5, 0.5).place(AREA, 100, 900, 10), (450, 0));
        assert_eq!(Placement::new(0.0, 0.0).place(AREA, 100, 900, 10), (10, 10));
    }

    #[test]
    fn window_wider_than_area_keeps_its_left_visible() {
        assert_eq!(Placement::new(1.0, 1.0).place(AREA, 1200, 50, 10), (0, 740));
    }

    #[test]
    fn values_are_clamped() {
        assert_eq!(Placement::new(-1.0, 2.0), Placement::new(0.0, 1.0));
        assert_eq!(Placement::new(f32::NAN, 0.3).x(), 0.5);
    }

    #[test]
    fn at_is_the_inverse_of_place() {
        let p = Placement::new(0.25, 0.75);
        let (x, y) = p.place(AREA, 100, 50, 10);
        let back = Placement::at_exact(AREA, 100.0, 50.0, 10.0, (x as f32, y as f32));
        assert!((back.x() - 0.25).abs() < 0.01 && (back.y() - 0.75).abs() < 0.01);
    }

    #[test]
    fn at_clamps_outside_the_free_space() {
        assert_eq!(
            Placement::at_exact(AREA, 100.0, 50.0, 10.0, (-50.0, 2000.0)),
            Placement::new(0.0, 1.0)
        );
        assert_eq!(
            Placement::at_exact(AREA, 2000.0, 50.0, 10.0, (0.0, 0.0)).x(),
            0.5
        );
    }
}
