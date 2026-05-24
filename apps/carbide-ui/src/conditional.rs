//! Conditional formatting — render-time cell formatting driven by the
//! cell's value.
//!
//! A [`Rule`] pairs a [`Condition`] with a format effect; when a cell's
//! value satisfies the condition, the rule's format is layered over the
//! cell's own. Pure — no egui, no engine — so `cargo test` covers it.

use serde::{Deserialize, Serialize};
use carbide_core::CellValue;

use crate::format::CellFormat;

/// A test applied to a cell's value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    /// A numeric value strictly greater than the threshold.
    GreaterThan(f64),
    /// A numeric value strictly less than the threshold.
    LessThan(f64),
    /// A numeric value equal to the threshold.
    EqualTo(f64),
    /// A numeric value not equal to the threshold.
    NotEqualTo(f64),
    /// A numeric value greater than or equal to the threshold.
    GreaterOrEqual(f64),
    /// A numeric value less than or equal to the threshold.
    LessOrEqual(f64),
    /// A numeric value within the two thresholds' inclusive range,
    /// whichever order the bounds are given in.
    Between(f64, f64),
    /// The cell's text contains this substring, compared
    /// case-insensitively. Only `CellValue::Text` cells can match.
    Contains(String),
    /// The cell holds boolean `TRUE`.
    IsTrue,
    /// The cell holds boolean `FALSE`.
    IsFalse,
    /// The cell is not empty.
    NonEmpty,
    /// The cell is empty.
    IsEmpty,
}

impl Condition {
    /// Whether `value` satisfies this condition.
    pub fn matches(&self, value: &CellValue) -> bool {
        let number = match value {
            CellValue::Number(n) => Some(*n),
            CellValue::Integer(i) => Some(*i as f64),
            _ => None,
        };
        match self {
            Condition::GreaterThan(t) => number.is_some_and(|n| n > *t),
            Condition::LessThan(t) => number.is_some_and(|n| n < *t),
            Condition::EqualTo(t) => number.is_some_and(|n| n == *t),
            Condition::NotEqualTo(t) => number.is_some_and(|n| n != *t),
            Condition::GreaterOrEqual(t) => number.is_some_and(|n| n >= *t),
            Condition::LessOrEqual(t) => number.is_some_and(|n| n <= *t),
            Condition::Between(a, b) => number.is_some_and(|n| n >= a.min(*b) && n <= a.max(*b)),
            Condition::Contains(needle) => match value {
                CellValue::Text(text) => text.to_lowercase().contains(&needle.to_lowercase()),
                _ => false,
            },
            Condition::IsTrue => matches!(value, CellValue::Bool(true)),
            Condition::IsFalse => matches!(value, CellValue::Bool(false)),
            Condition::NonEmpty => !matches!(value, CellValue::Empty),
            Condition::IsEmpty => matches!(value, CellValue::Empty),
        }
    }
}

/// A conditional-formatting rule: a condition, and the format effect to
/// layer on when a cell's value satisfies it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub condition: Condition,
    pub format: CellFormat,
}

/// The effective format for a cell — its manual `base` format with the
/// first matching rule's effect overlaid: bold/italic OR-ed in, colours
/// overriding when the rule sets them. Excel's "stop if true" — the
/// first matching rule wins, later rules are not applied.
pub fn effective_format(base: &CellFormat, value: &CellValue, rules: &[Rule]) -> CellFormat {
    let mut f = base.clone();
    for rule in rules {
        if rule.condition.matches(value) {
            let effect = &rule.format;
            f.bold = f.bold || effect.bold;
            f.italic = f.italic || effect.italic;
            f.strikethrough = f.strikethrough || effect.strikethrough;
            f.underline = f.underline || effect.underline;
            if effect.text_color.is_some() {
                f.text_color = effect.text_color;
            }
            if effect.fill.is_some() {
                f.fill = effect.fill;
            }
            break;
        }
    }
    f
}

/// The index pair to swap to move the rule at `index` one step toward
/// the front (`up`) or the back of a list of `len` rules — `None` when
/// that rule is already at the end it would move toward. Rules are
/// first-match-wins, so a move changes the rule's priority.
pub fn swap_for_move(len: usize, index: usize, up: bool) -> Option<(usize, usize)> {
    if index >= len {
        return None;
    }
    if up && index > 0 {
        Some((index, index - 1))
    } else if !up && index + 1 < len {
        Some((index, index + 1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    #[test]
    fn greater_less_equal_test_numeric_values() {
        let v = CellValue::Number(50.0);
        assert!(Condition::GreaterThan(10.0).matches(&v));
        assert!(!Condition::GreaterThan(99.0).matches(&v));
        assert!(Condition::LessThan(99.0).matches(&v));
        assert!(Condition::EqualTo(50.0).matches(&v));
        // Integers count as numbers.
        assert!(Condition::GreaterThan(2.0).matches(&CellValue::Integer(7)));
        // Non-numeric values never satisfy a numeric condition.
        assert!(!Condition::GreaterThan(0.0).matches(&CellValue::Text("hi".into())));
    }

    #[test]
    fn boolean_and_non_empty_conditions() {
        assert!(Condition::IsTrue.matches(&CellValue::Bool(true)));
        assert!(!Condition::IsTrue.matches(&CellValue::Bool(false)));
        assert!(Condition::IsFalse.matches(&CellValue::Bool(false)));
        assert!(Condition::NonEmpty.matches(&CellValue::Number(0.0)));
        assert!(!Condition::NonEmpty.matches(&CellValue::Empty));
    }

    #[test]
    fn not_equal_and_is_empty_conditions() {
        let v = CellValue::Number(50.0);
        assert!(Condition::NotEqualTo(10.0).matches(&v));
        assert!(!Condition::NotEqualTo(50.0).matches(&v));
        // Non-numeric values never satisfy a numeric condition.
        assert!(!Condition::NotEqualTo(0.0).matches(&CellValue::Text("hi".into())));
        // IsEmpty is the mirror of NonEmpty.
        assert!(Condition::IsEmpty.matches(&CellValue::Empty));
        assert!(!Condition::IsEmpty.matches(&CellValue::Number(0.0)));
    }

    #[test]
    fn ge_le_and_between_test_numeric_ranges() {
        let v = CellValue::Number(50.0);
        // >= and <= include the threshold itself.
        assert!(Condition::GreaterOrEqual(50.0).matches(&v));
        assert!(Condition::GreaterOrEqual(10.0).matches(&v));
        assert!(!Condition::GreaterOrEqual(51.0).matches(&v));
        assert!(Condition::LessOrEqual(50.0).matches(&v));
        assert!(!Condition::LessOrEqual(49.0).matches(&v));
        // Between is inclusive at both ends.
        assert!(Condition::Between(0.0, 50.0).matches(&v));
        assert!(Condition::Between(50.0, 100.0).matches(&v));
        assert!(!Condition::Between(0.0, 49.0).matches(&v));
        // The two bounds may be given in either order.
        assert!(Condition::Between(100.0, 0.0).matches(&v));
        // Integers count as numbers; non-numeric values never match.
        assert!(Condition::GreaterOrEqual(2.0).matches(&CellValue::Integer(7)));
        assert!(!Condition::Between(0.0, 9.0).matches(&CellValue::Text("hi".into())));
    }

    #[test]
    fn contains_tests_text_case_insensitively() {
        let urgent = CellValue::Text("Urgent: reply".to_string());
        assert!(Condition::Contains("urgent".to_string()).matches(&urgent));
        // Case-insensitive in both directions.
        assert!(Condition::Contains("REPLY".to_string()).matches(&urgent));
        // A substring that isn't present does not match.
        assert!(!Condition::Contains("done".to_string()).matches(&urgent));
        // Only text cells match — a number is never a Contains hit.
        assert!(!Condition::Contains("5".to_string()).matches(&CellValue::Number(50.0)));
        assert!(!Condition::Contains("x".to_string()).matches(&CellValue::Empty));
    }

    #[test]
    fn no_rules_leaves_the_base_format_untouched() {
        let base = CellFormat {
            bold: true,
            ..CellFormat::default()
        };
        let got = effective_format(&base, &CellValue::Number(1.0), &[]);
        assert_eq!(got, base);
    }

    #[test]
    fn a_matching_rule_overlays_its_fill() {
        let rule = Rule {
            condition: Condition::GreaterThan(100.0),
            format: CellFormat {
                fill: Some(Color32::RED),
                ..CellFormat::default()
            },
        };
        let hit = effective_format(
            &CellFormat::default(),
            &CellValue::Number(150.0),
            std::slice::from_ref(&rule),
        );
        assert_eq!(hit.fill, Some(Color32::RED));
        // A value that doesn't match leaves the format alone.
        let miss = effective_format(
            &CellFormat::default(),
            &CellValue::Number(50.0),
            std::slice::from_ref(&rule),
        );
        assert_eq!(miss.fill, None);
    }

    #[test]
    fn a_matching_rule_overlays_text_color() {
        let rule = Rule {
            condition: Condition::GreaterThan(0.0),
            format: CellFormat {
                text_color: Some(Color32::RED),
                ..CellFormat::default()
            },
        };
        let got = effective_format(
            &CellFormat::default(),
            &CellValue::Number(5.0),
            std::slice::from_ref(&rule),
        );
        assert_eq!(got.text_color, Some(Color32::RED));
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let rules = [
            Rule {
                condition: Condition::GreaterThan(0.0),
                format: CellFormat {
                    fill: Some(Color32::RED),
                    ..CellFormat::default()
                },
            },
            Rule {
                condition: Condition::GreaterThan(0.0),
                format: CellFormat {
                    fill: Some(Color32::BLUE),
                    ..CellFormat::default()
                },
            },
        ];
        let got = effective_format(&CellFormat::default(), &CellValue::Number(5.0), &rules);
        assert_eq!(got.fill, Some(Color32::RED));
    }

    #[test]
    fn a_matching_rule_adds_strikethrough_and_underline() {
        let rule = Rule {
            condition: Condition::GreaterThan(0.0),
            format: CellFormat {
                strikethrough: true,
                underline: true,
                ..CellFormat::default()
            },
        };
        // A plain cell gains both decorations from the matching rule.
        let got = effective_format(
            &CellFormat::default(),
            &CellValue::Number(5.0),
            std::slice::from_ref(&rule),
        );
        assert!(got.strikethrough && got.underline);
        // The rule only adds: a cell already struck through stays so
        // even when its value does not match the rule.
        let base = CellFormat {
            strikethrough: true,
            ..CellFormat::default()
        };
        let miss = effective_format(&base, &CellValue::Number(-1.0), std::slice::from_ref(&rule));
        assert!(miss.strikethrough && !miss.underline);
    }

    #[test]
    fn swap_for_move_finds_the_neighbour_or_stops_at_the_end() {
        // An interior rule swaps with the neighbour toward the move.
        assert_eq!(swap_for_move(4, 2, true), Some((2, 1)));
        assert_eq!(swap_for_move(4, 2, false), Some((2, 3)));
        // The first rule cannot move up; the last cannot move down.
        assert_eq!(swap_for_move(4, 0, true), None);
        assert_eq!(swap_for_move(4, 3, false), None);
        // A lone rule cannot move either way.
        assert_eq!(swap_for_move(1, 0, true), None);
        assert_eq!(swap_for_move(1, 0, false), None);
        // An out-of-range index yields nothing.
        assert_eq!(swap_for_move(3, 9, true), None);
    }
}
