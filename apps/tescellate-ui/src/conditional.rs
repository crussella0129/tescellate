//! Conditional formatting — render-time cell formatting driven by the
//! cell's value.
//!
//! A [`Rule`] pairs a [`Condition`] with a format effect; when a cell's
//! value satisfies the condition, the rule's format is layered over the
//! cell's own. Pure — no egui, no engine — so `cargo test` covers it.

use tescellate_core::CellValue;

use crate::format::CellFormat;

/// A test applied to a cell's value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Condition {
    /// A numeric value strictly greater than the threshold.
    GreaterThan(f64),
    /// A numeric value strictly less than the threshold.
    LessThan(f64),
    /// A numeric value equal to the threshold.
    EqualTo(f64),
    /// The cell holds boolean `TRUE`.
    IsTrue,
    /// The cell holds boolean `FALSE`.
    IsFalse,
    /// The cell is not empty.
    NonEmpty,
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
            Condition::IsTrue => matches!(value, CellValue::Bool(true)),
            Condition::IsFalse => matches!(value, CellValue::Bool(false)),
            Condition::NonEmpty => !matches!(value, CellValue::Empty),
        }
    }
}

/// A conditional-formatting rule: a condition, and the format effect to
/// layer on when a cell's value satisfies it.
#[derive(Debug, Clone, PartialEq)]
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
}
