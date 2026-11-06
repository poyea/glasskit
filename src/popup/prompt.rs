// Copyright (c) 2026 John Law
// SPDX-License-Identifier: MIT

use std::time::Duration;

/// How a choice button shows after a click. See [`Prompt::marks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// The button becomes green, for example the correct answer.
    Good,
    /// The button becomes red, for example a wrong answer.
    Bad,
}

/// What a [`ChoicePopup`](super::ChoicePopup) asks: a text, the choices, and how the popup
/// ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub(super) text: String,
    pub(super) choices: Vec<String>,
    /// For each choice that the user can click: the buttons to mark after that click.
    pub(super) marks: Option<Vec<Vec<(usize, Mark)>>>,
    pub(super) dismiss: String,
    pub(super) auto_dismiss: Option<Duration>,
}

impl Prompt {
    /// A prompt with this text and these choices, one button for each. The dismiss button
    /// says "Dismiss". A click on it closes the popup at once. The popup stays until the user
    /// clicks.
    pub fn new(
        text: impl Into<String>,
        choices: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            text: text.into(),
            choices: choices.into_iter().map(Into::into).collect(),
            marks: None,
            dismiss: "Dismiss".into(),
            auto_dismiss: None,
        }
    }

    /// Show a result after a click on a choice. `marks` gets the clicked index and returns
    /// the buttons to mark, as pairs of a choice index and a [`Mark`]. The popup shows the
    /// marks for [`PopupStyle::hold`], then fades out. Indexes out of range are ignored.
    ///
    /// For a quiz with the correct answer at index 1:
    ///
    /// ```
    /// # use glasskit::popup::{Mark, Prompt};
    /// let prompt = Prompt::new("2 + 2?", ["3", "4"]).marks(|clicked| {
    ///     let mut marks = vec![(1, Mark::Good)];
    ///     if clicked != 1 {
    ///         marks.push((clicked, Mark::Bad));
    ///     }
    ///     marks
    /// });
    /// ```
    #[must_use]
    pub fn marks(mut self, marks: impl Fn(usize) -> Vec<(usize, Mark)>) -> Self {
        let count = self.choices.len();
        self.marks = Some(
            (0..count)
                .map(|clicked| {
                    marks(clicked)
                        .into_iter()
                        .filter(|&(index, _)| index < count)
                        .collect()
                })
                .collect(),
        );
        self
    }

    /// The text of the dismiss button.
    #[must_use]
    pub fn dismiss_label(mut self, label: impl Into<String>) -> Self {
        self.dismiss = label.into();
        self
    }

    /// With `Some(after)`, the dismiss button counts down the seconds. The count stops while
    /// the pointer is on the popup. At zero, the popup fades out and gives
    /// [`Response::TimedOut`](super::Response::TimedOut). `None` turns the countdown off.
    #[must_use]
    pub fn auto_dismiss(mut self, after: Option<Duration>) -> Self {
        self.auto_dismiss = after;
        self
    }
}

/// How a [`ChoicePopup`](super::ChoicePopup) looks and moves. Sizes are in pixels at 100%
/// display scale. The popup scales them to the display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupStyle {
    /// The width of the popup. Its height follows from the content.
    pub width: f32,
    /// The size of the prompt text.
    pub text_size: f32,
    /// The size of the button text.
    pub button_text_size: f32,
    /// How long the popup takes to fade in, and to fade out after a countdown or marks.
    pub fade: Duration,
    /// How long the popup shows the marks of a click before it fades out.
    pub hold: Duration,
}

impl Default for PopupStyle {
    fn default() -> Self {
        Self {
            width: 360.0,
            text_size: 16.0,
            button_text_size: 14.0,
            fade: Duration::from_millis(300),
            hold: Duration::from_millis(1500),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_has_quiet_defaults() {
        let p = Prompt::new("Q?", ["a", "b"]);
        assert_eq!(p.choices, ["a", "b"]);
        assert_eq!(p.marks, None);
        assert_eq!(p.dismiss, "Dismiss");
        assert_eq!(p.auto_dismiss, None);
    }

    #[test]
    fn prompt_builders_set_each_part() {
        let p = Prompt::new("Q?", ["a", "b"])
            .marks(|clicked| vec![(clicked, Mark::Good)])
            .dismiss_label("Skip")
            .auto_dismiss(Some(Duration::from_secs(30)));
        assert_eq!(
            p.marks,
            Some(vec![vec![(0, Mark::Good)], vec![(1, Mark::Good)]])
        );
        assert_eq!(p.dismiss, "Skip");
        assert_eq!(p.auto_dismiss, Some(Duration::from_secs(30)));
    }

    #[test]
    fn prompt_takes_owned_strings() {
        let choices = vec![String::from("a"), String::from("b")];
        let p = Prompt::new(String::from("Q?"), choices).dismiss_label(String::from("Skip"));
        assert_eq!((p.text.as_str(), p.dismiss.as_str()), ("Q?", "Skip"));
        assert_eq!(p.choices, ["a", "b"]);
    }

    #[test]
    fn marks_out_of_range_are_dropped() {
        let p = Prompt::new("Q?", ["a", "b"]).marks(|_| vec![(0, Mark::Good), (5, Mark::Bad)]);
        assert_eq!(
            p.marks,
            Some(vec![vec![(0, Mark::Good)], vec![(0, Mark::Good)]])
        );
    }

    #[test]
    fn default_style_keeps_the_old_look() {
        let style = PopupStyle::default();
        assert_eq!(style.width, 360.0);
        assert_eq!((style.text_size, style.button_text_size), (16.0, 14.0));
        assert_eq!(style.fade, Duration::from_millis(300));
        assert_eq!(style.hold, Duration::from_millis(1500));
    }
}
