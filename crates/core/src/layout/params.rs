//! Layout analysis parameters.
//!
//! Contains LAParams struct for controlling layout analysis behavior.

/// Parameters for layout analysis.
///
/// Controls how characters are grouped into lines, words, and text boxes.
#[derive(Debug, Clone, PartialEq)]
pub struct LAParams {
    /// If two characters have more overlap than this they are considered to be
    /// on the same line. Specified relative to the minimum height of both characters.
    pub line_overlap: f64,

    /// If two characters are closer together than this margin they are considered
    /// part of the same line. Specified relative to the width of the character.
    pub char_margin: f64,

    /// If two lines are close together they are considered to be part of the
    /// same paragraph. Specified relative to the height of a line.
    pub line_margin: f64,

    /// If two characters on the same line are further apart than this margin then
    /// they are considered to be two separate words. Specified relative to the
    /// width of the character.
    pub word_margin: f64,

    /// Specifies how much horizontal and vertical position of text matters when
    /// determining order. Range: -1.0 (only horizontal) to +1.0 (only vertical).
    /// None disables advanced layout analysis.
    pub boxes_flow: Option<f64>,

    /// If vertical text should be considered during layout analysis.
    pub detect_vertical: bool,

    /// If layout analysis should be performed on text in figures.
    pub all_texts: bool,
}

impl Default for LAParams {
    fn default() -> Self {
        Self {
            line_overlap: 0.5,
            char_margin: 2.0,
            line_margin: 0.5,
            word_margin: 0.1,
            boxes_flow: Some(0.5),
            detect_vertical: false,
            all_texts: false,
        }
    }
}

impl LAParams {
    /// Creates new layout parameters with the specified values.
    ///
    /// # Panics
    /// Panics if boxes_flow is Some and not in range [-1.0, 1.0].
    pub fn new(
        line_overlap: f64,
        char_margin: f64,
        line_margin: f64,
        word_margin: f64,
        boxes_flow: Option<f64>,
        detect_vertical: bool,
        all_texts: bool,
    ) -> Self {
        if let Some(bf) = boxes_flow {
            assert!(
                (-1.0..=1.0).contains(&bf),
                "boxes_flow should be None, or a number between -1 and +1"
            );
        }

        Self {
            line_overlap,
            char_margin,
            line_margin,
            word_margin,
            boxes_flow,
            detect_vertical,
            all_texts,
        }
    }
}
