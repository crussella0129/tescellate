//! Lattice-agnostic helpers for editing a formula by clicking or
//! dragging across cells. Both the square and hex grids route their
//! formula-mode pointer interactions through these functions — the
//! lattice provides a way to address a cell (`address(coord) -> String`)
//! and a way to compare coordinates, and that is enough to assemble
//! `A1`, `A1:C3`, or the hex equivalents into the edit buffer.
//!
//! Stage A of the unified-lattice refactor: pure logic in one place,
//! callable from every grid renderer, so adding triangle / voronoi
//! grids later inherits the formula-edit interaction for free.

/// A formula-mode drag in progress: the start cell and the byte offset
/// in the edit buffer where the range text begins. Each subsequent
/// `dragged` frame truncates back to `buffer_anchor` and re-emits the
/// latest range string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragState<C: Copy> {
    pub start: C,
    pub buffer_anchor: usize,
}

/// The (start, end) cells of the last formula reference the user
/// pointed at, drawn as a marquee on the grid so they can see the
/// reference visually.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Highlight<C: Copy> {
    pub start: C,
    pub end: C,
}

/// Append a single-cell reference (e.g. `B2`) to `buffer` and signal a
/// re-focus via `fresh = true`. Returns the highlight to draw on the
/// grid until the edit is committed or cancelled.
pub fn click_insert<C: Copy>(
    buffer: &mut String,
    fresh: &mut bool,
    cell: C,
    address: impl Fn(C) -> String,
) -> Highlight<C> {
    buffer.push_str(&address(cell));
    *fresh = true;
    Highlight {
        start: cell,
        end: cell,
    }
}

/// Begin a formula-mode drag at `cell`: record the buffer's current
/// length as the anchor and append the start cell's address. Returns
/// both the drag-state to remember across frames and the initial
/// single-cell highlight.
pub fn drag_start<C: Copy>(
    buffer: &mut String,
    fresh: &mut bool,
    cell: C,
    address: impl Fn(C) -> String,
) -> (DragState<C>, Highlight<C>) {
    let buffer_anchor = buffer.len();
    buffer.push_str(&address(cell));
    *fresh = true;
    (
        DragState {
            start: cell,
            buffer_anchor,
        },
        Highlight {
            start: cell,
            end: cell,
        },
    )
}

/// Continue an in-progress drag: truncate the buffer to the recorded
/// anchor and write the current range — either the start address alone
/// (when the pointer hasn't moved off the start cell) or
/// `start:current` otherwise. Returns the highlight to draw.
pub fn drag_extend<C: Copy + PartialEq>(
    buffer: &mut String,
    fresh: &mut bool,
    drag: &DragState<C>,
    cell: C,
    address: impl Fn(C) -> String,
) -> Highlight<C> {
    let range = if cell == drag.start {
        address(drag.start)
    } else {
        format!("{}:{}", address(drag.start), address(cell))
    };
    buffer.truncate(drag.buffer_anchor);
    buffer.push_str(&range);
    *fresh = true;
    Highlight {
        start: drag.start,
        end: cell,
    }
}

/// Whether the buffer is in formula-edit mode — i.e. its first
/// non-whitespace character is `'='`. Both grids gate the
/// click/drag interaction on this.
pub fn is_formula_buffer(buffer: &str) -> bool {
    buffer.trim_start().starts_with('=')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_addr((c, r): (u32, u32)) -> String {
        format!("{}{}", (b'A' + c as u8) as char, r + 1)
    }

    #[test]
    fn click_insert_appends_a_single_cell_reference() {
        let mut buf = String::from("=");
        let mut fresh = false;
        let h = click_insert(&mut buf, &mut fresh, (1u32, 2u32), square_addr);
        assert_eq!(buf, "=B3");
        assert!(fresh);
        assert_eq!(h.start, h.end);
        assert_eq!(h.start, (1, 2));
    }

    #[test]
    fn drag_start_anchors_at_the_buffer_end_and_appends_the_start_cell() {
        let mut buf = String::from("=SUM(");
        let mut fresh = false;
        let (drag, h) = drag_start(&mut buf, &mut fresh, (0u32, 0u32), square_addr);
        assert_eq!(buf, "=SUM(A1");
        assert!(fresh);
        // The anchor is the length BEFORE the address was appended, so
        // `drag_extend` can rewrite from there.
        assert_eq!(drag.buffer_anchor, "=SUM(".len());
        assert_eq!(h.start, h.end);
    }

    #[test]
    fn drag_extend_overwrites_the_range_from_the_anchor() {
        let mut buf = String::from("=SUM(A1");
        let mut fresh = false;
        let drag = DragState {
            start: (0u32, 0u32),
            buffer_anchor: "=SUM(".len(),
        };
        let h = drag_extend(&mut buf, &mut fresh, &drag, (2, 3), square_addr);
        assert_eq!(buf, "=SUM(A1:C4");
        assert!(fresh);
        assert_eq!(h.start, (0, 0));
        assert_eq!(h.end, (2, 3));
    }

    #[test]
    fn drag_extend_collapses_to_a_single_cell_when_the_pointer_hasnt_moved() {
        let mut buf = String::from("=SUM(A1");
        let mut fresh = false;
        let drag = DragState {
            start: (0u32, 0u32),
            buffer_anchor: "=SUM(".len(),
        };
        let h = drag_extend(&mut buf, &mut fresh, &drag, (0, 0), square_addr);
        // Same start and end — emit just the address, not "A1:A1".
        assert_eq!(buf, "=SUM(A1");
        assert_eq!(h.start, h.end);
    }

    #[test]
    fn is_formula_buffer_recognises_an_equals_prefix_and_tolerates_leading_whitespace() {
        assert!(is_formula_buffer("=A1+B2"));
        assert!(is_formula_buffer("  =A1"));
        assert!(!is_formula_buffer(""));
        assert!(!is_formula_buffer("123"));
        assert!(!is_formula_buffer("hello"));
        assert!(!is_formula_buffer(" hello"));
    }
}
