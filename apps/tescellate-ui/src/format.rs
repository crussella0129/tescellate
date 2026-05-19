//! The pure cell-formatting model — visual style and number formats.
//!
//! No egui rendering and no engine: just the data and the number-format
//! logic, so it is unit-tested with ordinary `cargo test`. `app.rs` reads
//! a [`CellFormat`] when painting a cell and through [`render_number`]
//! when turning a value into display text.

use std::collections::HashMap;

use egui::Color32;
use tescellate_tess::hex::HexCoord;

/// Horizontal text alignment within a cell. `Auto` defers to the cell's
/// value type at render time — see [`effective_align`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    /// Right for numbers, left for everything else — the default.
    #[default]
    Auto,
    Left,
    Center,
    Right,
}

/// The alignment a cell actually renders with. An explicit `Left`,
/// `Center`, or `Right` is used as-is; `Auto` resolves to `Right` for a
/// numeric value and `Left` otherwise — the spreadsheet default.
pub fn effective_align(align: HAlign, numeric: bool) -> HAlign {
    match align {
        HAlign::Auto if numeric => HAlign::Right,
        HAlign::Auto => HAlign::Left,
        other => other,
    }
}

/// How a numeric cell value is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NumberFormat {
    /// The engine's natural rendering.
    #[default]
    General,
    /// A fixed number of decimal places.
    Number { decimals: u8 },
    /// A percentage — the value times 100, with a `%` suffix.
    Percent { decimals: u8 },
    /// A `$` prefix and two decimal places.
    Currency,
    /// Fixed decimals with comma thousands separators in the integer part.
    Thousands { decimals: u8 },
    /// Scientific notation — a mantissa and a power-of-ten exponent.
    Scientific { decimals: u8 },
    /// A calendar date — the value, truncated to whole days, read as a
    /// count of days since 1970-01-01 and shown as `YYYY-MM-DD`.
    Date,
    /// A time of day — the value's fractional part (a fraction of a
    /// day) shown as `HH:MM:SS`.
    Time,
}

/// Which sides of a cell carry a border line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Borders {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

/// A border command from the ribbon, applied across the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderMode {
    /// Border all four sides of every selected cell.
    All,
    /// Border only the sides on the selection's outer edge.
    Outer,
    /// Clear every border.
    None,
}

/// Which sides of `cell` get a border under `mode`, given the selection's
/// `bounds` — its min and max `(col, row)` corner. `All` borders every
/// side; `None` clears them; `Outer` borders only the sides lying on the
/// selection's edge (so a one-cell selection gets all four).
pub fn border_sides(
    cell: (u32, u32),
    bounds: ((u32, u32), (u32, u32)),
    mode: BorderMode,
) -> Borders {
    match mode {
        BorderMode::None => Borders::default(),
        BorderMode::All => Borders {
            top: true,
            bottom: true,
            left: true,
            right: true,
        },
        BorderMode::Outer => {
            let ((min_c, min_r), (max_c, max_r)) = bounds;
            let (c, r) = cell;
            Borders {
                top: r == min_r,
                bottom: r == max_r,
                left: c == min_c,
                right: c == max_c,
            }
        }
    }
}

/// Which of a hexagon's six edges carry a border line. Edge `i` is the
/// segment from hex vertex `i` to vertex `i + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HexBorders {
    pub edges: [bool; 6],
}

impl HexBorders {
    /// Every edge bordered.
    pub fn all() -> Self {
        Self { edges: [true; 6] }
    }
}

/// The six axial neighbour offsets, indexed by hex edge: offset `i` is
/// the hex sharing edge `i` — the segment from vertex `i` to vertex
/// `i + 1`. The mapping is orientation-independent: a `HexLattice`'s
/// `vertices` and `centroid` rotate together, so pointy- and flat-top
/// hexes share it.
const HEX_EDGE_NEIGHBORS: [(i32, i32); 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// The hex sharing edge `edge` of `cell` — the neighbour across that
/// edge. `edge` is taken modulo 6.
pub fn hex_edge_neighbor(cell: HexCoord, edge: usize) -> HexCoord {
    let (dq, dr) = HEX_EDGE_NEIGHBORS[edge % 6];
    HexCoord::new(cell.q + dq, cell.r + dr)
}

/// Which of `cell`'s six edges lie on the perimeter of a selection: an
/// edge is on the perimeter when the hex across it is not itself
/// selected. `selected` reports selection membership. A lone selected
/// hex gets all six edges; a hex whose six neighbours are all selected
/// gets none — the hex analogue of [`border_sides`] under
/// [`BorderMode::Outer`].
pub fn hex_outer_borders(cell: HexCoord, selected: impl Fn(HexCoord) -> bool) -> HexBorders {
    let mut edges = [false; 6];
    for (edge, on) in edges.iter_mut().enumerate() {
        *on = !selected(hex_edge_neighbor(cell, edge));
    }
    HexBorders { edges }
}

/// The full visual format of one cell. The `Default` is "no formatting".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellFormat {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub align: HAlign,
    pub text_color: Option<Color32>,
    pub fill: Option<Color32>,
    pub number: NumberFormat,
    pub borders: Borders,
    pub hex_borders: HexBorders,
}

impl CellFormat {
    /// Whether this format carries no styling at all.
    pub fn is_default(&self) -> bool {
        *self == CellFormat::default()
    }
}

/// Render a numeric value under a number format. `None` means "use the
/// engine's natural rendering" — returned for [`NumberFormat::General`].
pub fn render_number(value: f64, format: NumberFormat) -> Option<String> {
    match format {
        NumberFormat::General => None,
        NumberFormat::Number { decimals } => Some(format!("{:.*}", decimals as usize, value)),
        NumberFormat::Percent { decimals } => {
            Some(format!("{:.*}%", decimals as usize, value * 100.0))
        }
        NumberFormat::Currency => Some(format!("${:.2}", value)),
        NumberFormat::Thousands { decimals } => Some(group_thousands(value, decimals)),
        NumberFormat::Scientific { decimals } => Some(format!("{:.*e}", decimals as usize, value)),
        NumberFormat::Date => Some(render_date(value)),
        NumberFormat::Time => Some(render_time(value)),
    }
}

/// Render `value`, truncated to whole days, as a `YYYY-MM-DD` date —
/// the day count measured from the 1970-01-01 epoch (day 0).
fn render_date(value: f64) -> String {
    let (y, m, d) = civil_from_days(value.trunc() as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The calendar date `days` days after 1970-01-01, as `(year, month,
/// day)` — Howard Hinnant's `civil_from_days`, exact for the proleptic
/// Gregorian calendar with no lookup tables.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m, d)
}

/// Render `value`'s fractional part — a fraction of a day — as a
/// `HH:MM:SS` time of day. The whole-day part is ignored; a negative
/// value wraps so its time of day stays within the day.
fn render_time(value: f64) -> String {
    let secs = (value.rem_euclid(1.0) * 86_400.0).round() as u32 % 86_400;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// Render `value` with `decimals` decimal places and comma separators
/// every three digits of the integer part — `1234567.5 -> "1,234,567.5"`.
fn group_thousands(value: f64, decimals: u8) -> String {
    let formatted = format!("{:.*}", decimals as usize, value);
    let (sign, rest) = match formatted.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", formatted.as_str()),
    };
    let (integer, fraction) = match rest.split_once('.') {
        Some((int, frac)) => (int, Some(frac)),
        None => (rest, None),
    };
    let grouped = group_digits(integer);
    match fraction {
        Some(frac) => format!("{sign}{grouped}.{frac}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Insert a comma before every group of three digits, counted from the
/// right — `"1234567" -> "1,234,567"`.
fn group_digits(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

/// Increase (`delta > 0`) or decrease (`delta < 0`) a number format's
/// decimal-place count, clamped to `0..=15`. `Number`, `Percent`,
/// `Thousands`, and `Scientific` carry a count and shift it; `General`
/// gains an explicit `Number` count the first time decimals are added;
/// `Currency` (and `General` with nothing to drop) are returned as-is.
pub fn adjust_decimals(format: NumberFormat, delta: i32) -> NumberFormat {
    let bump = |d: u8| (d as i32 + delta).clamp(0, 15) as u8;
    match format {
        NumberFormat::Number { decimals } => NumberFormat::Number {
            decimals: bump(decimals),
        },
        NumberFormat::Percent { decimals } => NumberFormat::Percent {
            decimals: bump(decimals),
        },
        NumberFormat::Thousands { decimals } => NumberFormat::Thousands {
            decimals: bump(decimals),
        },
        NumberFormat::Scientific { decimals } => NumberFormat::Scientific {
            decimals: bump(decimals),
        },
        // Adding decimals to General turns it into a fixed Number format.
        NumberFormat::General if delta > 0 => NumberFormat::Number {
            decimals: delta.clamp(0, 15) as u8,
        },
        // General with nothing to drop, and Currency, are left untouched.
        other => other,
    }
}

/// Per-cell formatting overrides, keyed by a lattice's coordinate type
/// `K` — `(u32, u32)` for the square sheet, `HexCoord` for the hex sheet.
/// A cell absent from the map is unstyled, so an empty map is a plain
/// sheet.
#[derive(Debug, Clone)]
pub struct FormatMap<K> {
    formats: HashMap<K, CellFormat>,
}

impl<K> Default for FormatMap<K> {
    fn default() -> Self {
        Self {
            formats: HashMap::new(),
        }
    }
}

impl<K: Eq + std::hash::Hash + Copy> FormatMap<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The format of a cell — its default if never styled.
    pub fn get(&self, cell: K) -> CellFormat {
        self.formats.get(&cell).cloned().unwrap_or_default()
    }

    /// Mutate a cell's format in place. An entry that ends up back at the
    /// default is dropped, so the map only ever holds real styling.
    pub fn update(&mut self, cell: K, edit: impl FnOnce(&mut CellFormat)) {
        let mut format = self.get(cell);
        edit(&mut format);
        if format.is_default() {
            self.formats.remove(&cell);
        } else {
            self.formats.insert(cell, format);
        }
    }

    /// How many cells carry non-default formatting.
    pub fn styled_count(&self) -> usize {
        self.formats.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_defers_to_natural_rendering() {
        assert_eq!(render_number(3.5, NumberFormat::General), None);
    }

    #[test]
    fn number_format_fixes_the_decimal_places() {
        assert_eq!(
            render_number(3.5, NumberFormat::Number { decimals: 2 }),
            Some("3.50".to_string()),
        );
        assert_eq!(
            render_number(3.7, NumberFormat::Number { decimals: 0 }),
            Some("4".to_string()),
        );
    }

    #[test]
    fn percent_scales_by_one_hundred() {
        assert_eq!(
            render_number(0.5, NumberFormat::Percent { decimals: 0 }),
            Some("50%".to_string()),
        );
        assert_eq!(
            render_number(0.125, NumberFormat::Percent { decimals: 1 }),
            Some("12.5%".to_string()),
        );
    }

    #[test]
    fn currency_prefixes_a_dollar_sign() {
        assert_eq!(
            render_number(1200.0, NumberFormat::Currency),
            Some("$1200.00".to_string()),
        );
    }

    #[test]
    fn thousands_groups_the_integer_part() {
        assert_eq!(
            render_number(1234567.5, NumberFormat::Thousands { decimals: 1 }),
            Some("1,234,567.5".to_string()),
        );
        // Fewer than four digits get no separator.
        assert_eq!(
            render_number(950.0, NumberFormat::Thousands { decimals: 0 }),
            Some("950".to_string()),
        );
        // Exactly four digits get one separator.
        assert_eq!(
            render_number(1000.0, NumberFormat::Thousands { decimals: 0 }),
            Some("1,000".to_string()),
        );
        // The sign stays outside the grouping.
        assert_eq!(
            render_number(-12345.0, NumberFormat::Thousands { decimals: 0 }),
            Some("-12,345".to_string()),
        );
    }

    #[test]
    fn scientific_uses_exponent_notation() {
        assert_eq!(
            render_number(1234567.0, NumberFormat::Scientific { decimals: 2 }),
            Some("1.23e6".to_string()),
        );
        assert_eq!(
            render_number(0.0042, NumberFormat::Scientific { decimals: 1 }),
            Some("4.2e-3".to_string()),
        );
    }

    #[test]
    fn date_format_reads_the_value_as_a_day_count() {
        let date = |n: f64| render_number(n, NumberFormat::Date);
        // Day 0 is the 1970-01-01 epoch.
        assert_eq!(date(0.0), Some("1970-01-01".to_string()));
        // A month on, then a (non-leap) year on.
        assert_eq!(date(31.0), Some("1970-02-01".to_string()));
        assert_eq!(date(365.0), Some("1971-01-01".to_string()));
        // 1972 is a leap year — day 789 is its 29th of February.
        assert_eq!(date(789.0), Some("1972-02-29".to_string()));
        // Days before the epoch run negative.
        assert_eq!(date(-1.0), Some("1969-12-31".to_string()));
        // A fractional value is truncated to whole days.
        assert_eq!(date(31.9), Some("1970-02-01".to_string()));
    }

    #[test]
    fn time_format_reads_the_fractional_day() {
        let time = |n: f64| render_number(n, NumberFormat::Time);
        // The fraction of a day: 0 is midnight, 0.5 is noon.
        assert_eq!(time(0.0), Some("00:00:00".to_string()));
        assert_eq!(time(0.5), Some("12:00:00".to_string()));
        assert_eq!(time(0.25), Some("06:00:00".to_string()));
        assert_eq!(time(0.75), Some("18:00:00".to_string()));
        // The whole-day part is ignored.
        assert_eq!(time(1.5), Some("12:00:00".to_string()));
        // A negative value wraps to a time within the day.
        assert_eq!(time(-0.25), Some("18:00:00".to_string()));
        // One minute and one second past midnight.
        assert_eq!(time(61.0 / 86_400.0), Some("00:01:01".to_string()));
    }

    #[test]
    fn adjust_decimals_bumps_the_count() {
        assert_eq!(
            adjust_decimals(NumberFormat::Number { decimals: 2 }, 1),
            NumberFormat::Number { decimals: 3 },
        );
        assert_eq!(
            adjust_decimals(NumberFormat::Percent { decimals: 2 }, -1),
            NumberFormat::Percent { decimals: 1 },
        );
        assert_eq!(
            adjust_decimals(NumberFormat::Thousands { decimals: 0 }, 2),
            NumberFormat::Thousands { decimals: 2 },
        );
    }

    #[test]
    fn adjust_decimals_clamps_to_zero_and_fifteen() {
        // Can't drop below zero.
        assert_eq!(
            adjust_decimals(NumberFormat::Number { decimals: 0 }, -1),
            NumberFormat::Number { decimals: 0 },
        );
        // Can't exceed fifteen.
        assert_eq!(
            adjust_decimals(NumberFormat::Scientific { decimals: 15 }, 1),
            NumberFormat::Scientific { decimals: 15 },
        );
    }

    #[test]
    fn adjust_decimals_gives_general_an_explicit_count() {
        // Adding a decimal to General turns it into a fixed Number format.
        assert_eq!(
            adjust_decimals(NumberFormat::General, 1),
            NumberFormat::Number { decimals: 1 },
        );
        // Removing from General is a no-op — it has no decimals to drop.
        assert_eq!(
            adjust_decimals(NumberFormat::General, -1),
            NumberFormat::General,
        );
    }

    #[test]
    fn adjust_decimals_leaves_currency_alone() {
        assert_eq!(
            adjust_decimals(NumberFormat::Currency, 1),
            NumberFormat::Currency,
        );
        assert_eq!(
            adjust_decimals(NumberFormat::Currency, -1),
            NumberFormat::Currency,
        );
    }

    #[test]
    fn cell_format_default_is_unstyled() {
        assert!(CellFormat::default().is_default());
        let f = CellFormat {
            bold: true,
            ..Default::default()
        };
        assert!(!f.is_default());
    }

    #[test]
    fn format_map_stores_and_evicts() {
        let mut map = FormatMap::new();
        assert_eq!(map.get((0, 0)), CellFormat::default());

        map.update((2, 3), |f| f.bold = true);
        assert!(map.get((2, 3)).bold);
        assert_eq!(map.styled_count(), 1);

        // Toggling back to the default drops the entry.
        map.update((2, 3), |f| f.bold = false);
        assert!(map.get((2, 3)).is_default());
        assert_eq!(map.styled_count(), 0);
    }

    #[test]
    fn format_map_update_composes() {
        let mut map = FormatMap::new();
        map.update((1, 1), |f| f.italic = true);
        map.update((1, 1), |f| f.align = HAlign::Right);
        let f = map.get((1, 1));
        assert!(f.italic);
        assert_eq!(f.align, HAlign::Right);
    }

    #[test]
    fn effective_align_resolves_auto_by_value_type() {
        // Auto becomes Right for a number, Left for anything else.
        assert_eq!(effective_align(HAlign::Auto, true), HAlign::Right);
        assert_eq!(effective_align(HAlign::Auto, false), HAlign::Left);
        // An explicit choice is passed through regardless of the value.
        assert_eq!(effective_align(HAlign::Left, true), HAlign::Left);
        assert_eq!(effective_align(HAlign::Center, true), HAlign::Center);
        assert_eq!(effective_align(HAlign::Right, false), HAlign::Right);
    }

    #[test]
    fn border_sides_all_and_none() {
        let bounds = ((1, 1), (3, 3));
        let all = border_sides((2, 2), bounds, BorderMode::All);
        assert!(all.top && all.bottom && all.left && all.right);
        assert_eq!(
            border_sides((2, 2), bounds, BorderMode::None),
            Borders::default(),
        );
    }

    #[test]
    fn outer_borders_trace_the_selection_edge() {
        let bounds = ((1, 1), (3, 3));
        // Top-left corner — top and left only.
        let tl = border_sides((1, 1), bounds, BorderMode::Outer);
        assert!(tl.top && tl.left && !tl.bottom && !tl.right);
        // Bottom-right corner — bottom and right only.
        let br = border_sides((3, 3), bounds, BorderMode::Outer);
        assert!(br.bottom && br.right && !br.top && !br.left);
        // A top-edge, non-corner cell — top only.
        let top = border_sides((2, 1), bounds, BorderMode::Outer);
        assert!(top.top && !top.bottom && !top.left && !top.right);
        // An interior cell — no borders.
        assert_eq!(
            border_sides((2, 2), bounds, BorderMode::Outer),
            Borders::default(),
        );
    }

    #[test]
    fn outer_borders_on_a_single_cell_are_all_four() {
        let only = border_sides((5, 5), ((5, 5), (5, 5)), BorderMode::Outer);
        assert!(only.top && only.bottom && only.left && only.right);
    }

    #[test]
    fn format_map_works_with_a_hex_key() {
        let mut map: FormatMap<HexCoord> = FormatMap::new();
        let cell = HexCoord::new(1, -2);
        map.update(cell, |f| f.bold = true);
        assert!(map.get(cell).bold);
        assert_eq!(map.styled_count(), 1);
        // An unstyled hex cell reads as the default format.
        assert!(map.get(HexCoord::new(0, 0)).is_default());
    }

    #[test]
    fn hex_borders_all_and_default() {
        assert_eq!(HexBorders::all().edges, [true; 6]);
        assert_eq!(HexBorders::default().edges, [false; 6]);
    }

    #[test]
    fn hex_edge_neighbors_are_six_distinct_adjacent_hexes() {
        let cell = HexCoord::new(2, -3);
        let mut seen: Vec<HexCoord> = Vec::new();
        for edge in 0..6 {
            let n = hex_edge_neighbor(cell, edge);
            // Each edge's neighbour is exactly one axial step away.
            assert_eq!(cell.distance(n), 1, "edge {edge} neighbour not adjacent");
            assert!(!seen.contains(&n), "edge {edge} neighbour duplicated");
            seen.push(n);
        }
        assert_eq!(seen.len(), 6);
    }

    #[test]
    fn hex_edge_neighbor_index_wraps_modulo_six() {
        let cell = HexCoord::new(0, 0);
        assert_eq!(hex_edge_neighbor(cell, 6), hex_edge_neighbor(cell, 0));
        assert_eq!(hex_edge_neighbor(cell, 7), hex_edge_neighbor(cell, 1));
    }

    #[test]
    fn lone_hex_gets_every_outer_edge() {
        let cell = HexCoord::new(1, 1);
        // Only `cell` itself is selected.
        let b = hex_outer_borders(cell, |c| c == cell);
        assert_eq!(b.edges, [true; 6]);
    }

    #[test]
    fn fully_surrounded_hex_gets_no_outer_edge() {
        let cell = HexCoord::new(0, 0);
        // Every cell within one step is selected — `cell` is interior.
        let b = hex_outer_borders(cell, |c| cell.distance(c) <= 1);
        assert_eq!(b.edges, [false; 6]);
    }

    #[test]
    fn outer_edge_is_set_exactly_where_the_neighbour_is_unselected() {
        let cell = HexCoord::new(0, 0);
        // Select `cell` plus only its edge-2 and edge-5 neighbours.
        let keep = [cell, hex_edge_neighbor(cell, 2), hex_edge_neighbor(cell, 5)];
        let b = hex_outer_borders(cell, |c| keep.contains(&c));
        // Edges 2 and 5 face a selected hex, so they are not perimeter.
        assert_eq!(b.edges, [true, true, false, true, true, false]);
    }

    #[test]
    fn outer_borders_trace_a_two_hex_selection() {
        let a = HexCoord::new(0, 0);
        let b = hex_edge_neighbor(a, 0); // a's edge-0 neighbour
        let selected = [a, b];
        let in_sel = |c: HexCoord| selected.contains(&c);
        // `a`: every edge perimeter except edge 0, which faces `b`.
        assert_eq!(
            hex_outer_borders(a, in_sel).edges,
            [false, true, true, true, true, true],
        );
        // `b`: every edge perimeter except edge 3, which faces `a`.
        assert_eq!(
            hex_outer_borders(b, in_sel).edges,
            [true, true, true, false, true, true],
        );
    }
}
