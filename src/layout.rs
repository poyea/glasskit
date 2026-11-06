// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

//! Rows and columns for a [`Dialog`](crate::dialog::Dialog), so that its controls need no
//! hand-placed coordinates.
//!
//! ```
//! use glasskit::dialog::Bounds;
//! use glasskit::layout::{Column, Size};
//!
//! let mut column = Column::new(Bounds::new(0, 0, 400, 300), 8);
//! let [label, field] = column
//!     .next(26)
//!     .columns([Size::Fixed(120), Size::Fill(1)], 8);
//! assert_eq!(label, Bounds::new(0, 0, 120, 26));
//! assert_eq!(field, Bounds::new(128, 0, 272, 26));
//! assert_eq!(column.next(26).y, 34);
//! ```

use crate::dialog::Bounds;

/// The width of one column in [`Bounds::columns`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// Exactly this many pixels.
    Fixed(i32),
    /// A share of the width that the fixed columns and the gaps leave. A column with
    /// `Fill(2)` gets twice the width of a column with `Fill(1)`.
    Fill(u32),
}

impl Bounds {
    /// The right edge.
    pub const fn right(self) -> i32 {
        self.x + self.width
    }

    /// The bottom edge.
    pub const fn bottom(self) -> i32 {
        self.y + self.height
    }

    /// These bounds, smaller by `by` on each side. The size does not go below 0.
    #[must_use]
    pub fn inset(self, by: i32) -> Self {
        Self::new(
            self.x + by,
            self.y + by,
            (self.width - 2 * by).max(0),
            (self.height - 2 * by).max(0),
        )
    }

    /// Split the width into columns with `gap` pixels between them. The columns keep the top
    /// edge and the height. Fill columns share the space that is left; the last fill column
    /// gets the pixels that rounding leaves, so the columns end exactly at the right edge.
    pub fn columns<const N: usize>(self, sizes: [Size; N], gap: i32) -> [Bounds; N] {
        let fixed: i32 = sizes
            .iter()
            .map(|s| match s {
                Size::Fixed(w) => *w,
                Size::Fill(_) => 0,
            })
            .sum();
        let weights: u32 = sizes
            .iter()
            .map(|s| match s {
                Size::Fill(w) => *w,
                Size::Fixed(_) => 0,
            })
            .sum();
        let gaps = gap * (N.saturating_sub(1) as i32);
        let free = (self.width - fixed - gaps).max(0);
        let last_fill = sizes
            .iter()
            .rposition(|s| matches!(s, Size::Fill(w) if *w > 0));
        let mut given = 0;
        let mut x = self.x;
        let mut out = [self; N];
        for (i, size) in sizes.iter().enumerate() {
            let width = match *size {
                Size::Fixed(w) => w,
                Size::Fill(_) if Some(i) == last_fill => free - given,
                Size::Fill(w) if weights > 0 => {
                    let share = (i64::from(free) * i64::from(w) / i64::from(weights)) as i32;
                    given += share;
                    share
                }
                Size::Fill(_) => 0,
            };
            out[i] = Bounds::new(x, self.y, width, self.height);
            x += width + gap;
        }
        out
    }
}

/// Rows from the top of an area down, with a gap between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    area: Bounds,
    gap: i32,
    y: i32,
}

impl Column {
    /// Rows in `area`, with `gap` pixels between two rows.
    pub fn new(area: Bounds, gap: i32) -> Self {
        Self {
            area,
            gap,
            y: area.y,
        }
    }

    /// The next row: the full width of the area, and `height` pixels high.
    pub fn next(&mut self, height: i32) -> Bounds {
        let row = Bounds::new(self.area.x, self.y, self.area.width, height);
        self.y += height + self.gap;
        row
    }

    /// Leave `height` pixels empty, as if a row was there.
    pub fn skip(&mut self, height: i32) {
        self.y += height + self.gap;
    }

    /// The space below the rows given so far, to the bottom of the area.
    pub fn rest(&self) -> Bounds {
        Bounds::new(
            self.area.x,
            self.y,
            self.area.width,
            (self.area.bottom() - self.y).max(0),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Bounds = Bounds::new(10, 20, 400, 300);

    #[test]
    fn rows_go_down_with_the_gap() {
        let mut c = Column::new(AREA, 8);
        assert_eq!(c.next(26), Bounds::new(10, 20, 400, 26));
        assert_eq!(c.next(30), Bounds::new(10, 54, 400, 30));
        c.skip(10);
        assert_eq!(c.next(20).y, 110);
        assert_eq!(c.rest(), Bounds::new(10, 138, 400, 182));
    }

    #[test]
    fn rest_is_empty_below_the_area() {
        let mut c = Column::new(Bounds::new(0, 0, 100, 50), 0);
        c.next(80);
        assert_eq!(c.rest().height, 0);
    }

    #[test]
    fn columns_share_the_free_width() {
        let [a, b, c] = AREA.columns([Size::Fixed(100), Size::Fill(1), Size::Fill(1)], 10);
        assert_eq!(a, Bounds::new(10, 20, 100, 300));
        assert_eq!(b, Bounds::new(120, 20, 140, 300));
        assert_eq!(c, Bounds::new(270, 20, 140, 300));
        assert_eq!(c.right(), AREA.right());
    }

    #[test]
    fn weights_split_by_ratio_and_end_at_the_edge() {
        let [a, b] = Bounds::new(0, 0, 301, 10).columns([Size::Fill(1), Size::Fill(2)], 0);
        assert_eq!((a.width, b.width), (100, 201));
        assert_eq!(b.right(), 301);
    }

    #[test]
    fn fixed_columns_alone_keep_their_widths() {
        let [a, b] = AREA.columns([Size::Fixed(50), Size::Fixed(60)], 5);
        assert_eq!((a.x, a.width, b.x, b.width), (10, 50, 65, 60));
    }

    #[test]
    fn too_little_space_gives_fill_columns_zero_width() {
        let [a, b] = Bounds::new(0, 0, 50, 10).columns([Size::Fixed(80), Size::Fill(1)], 0);
        assert_eq!((a.width, b.width), (80, 0));
    }

    #[test]
    fn inset_shrinks_each_side() {
        assert_eq!(AREA.inset(16), Bounds::new(26, 36, 368, 268));
        assert_eq!(Bounds::new(0, 0, 10, 10).inset(8), Bounds::new(8, 8, 0, 0));
    }
}
