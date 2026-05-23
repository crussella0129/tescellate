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

/// A pointer event over a lattice cell, translated from raw egui flags.
/// `Idle` covers "nothing happened this frame" — the common case during
/// a formula edit when the user isn't interacting with the grid.
///
/// Each `draw_*_grid` builds this from `response.{clicked, drag_started,
/// dragged, drag_stopped}()` via [`event_from_response`] and feeds it to
/// [`dispatch`] (ADR-013 shared interaction helper).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event<C: Copy> {
    Idle,
    Clicked(C),
    DragStarted(C),
    Dragged(C),
    DragStopped,
}

/// Translate egui's per-frame response flags into a single [`Event`].
/// Pure (no egui types in the signature) so each lattice's call-site
/// translation is unit-testable without an egui context. Priority order
/// (rarely simultaneous, but specified for completeness):
/// `DragStarted` > `Dragged` > `DragStopped` > `Clicked` > `Idle`.
///
/// `cell = None` falls back to `Idle` for cell-bearing variants — a
/// click that landed off any cell shouldn't fire a phantom Clicked.
pub fn event_from_response<C: Copy>(
    clicked: bool,
    drag_started: bool,
    dragged: bool,
    drag_stopped: bool,
    cell: Option<C>,
) -> Event<C> {
    if drag_started {
        return cell.map(Event::DragStarted).unwrap_or(Event::Idle);
    }
    if dragged {
        return cell.map(Event::Dragged).unwrap_or(Event::Idle);
    }
    if drag_stopped {
        return Event::DragStopped;
    }
    if clicked {
        return cell.map(Event::Clicked).unwrap_or(Event::Idle);
    }
    Event::Idle
}

/// Dispatch an [`Event`] through the formula-mode click/drag pipeline.
/// Returns `(new_drag, new_highlight)`: the caller assigns both back into
/// its `Sheet<C>` bundle. When the buffer isn't in formula mode (no `=`
/// prefix) or the event is `Idle`, returns `(current_drag, None)` — leave
/// the drag state untouched and don't mutate the buffer.
///
/// Centralises the `if is_formula_buffer { drag_started/dragged/clicked
/// branches }` block that the square/hex/triangle `draw_*_grid` paths
/// each used to inline. Voronoi now joins via the same single call.
pub fn dispatch<C: Copy + PartialEq>(
    buffer: &mut String,
    fresh: &mut bool,
    current_drag: Option<DragState<C>>,
    event: Event<C>,
    address: impl Fn(C) -> String,
) -> (Option<DragState<C>>, Option<Highlight<C>>) {
    if !is_formula_buffer(buffer) {
        return (current_drag, None);
    }
    match event {
        Event::Idle => (current_drag, None),
        Event::DragStarted(c) => {
            let (drag, h) = drag_start(buffer, fresh, c, address);
            (Some(drag), Some(h))
        }
        Event::Dragged(c) => {
            if let Some(drag) = current_drag {
                let h = drag_extend(buffer, fresh, &drag, c, address);
                (Some(drag), Some(h))
            } else {
                // A `dragged` event with no prior `drag_started` — treat as
                // an in-flight extend without a drag state, fall back to a
                // single-cell insert so the cell address still lands in the
                // buffer rather than getting lost.
                let h = click_insert(buffer, fresh, c, address);
                (None, Some(h))
            }
        }
        Event::DragStopped => (None, None),
        Event::Clicked(c) => {
            let h = click_insert(buffer, fresh, c, address);
            (None, Some(h))
        }
    }
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

    // --- T-002: dispatch ---

    #[test]
    fn dispatch_noop_when_not_formula() {
        let mut buf = String::from("hello");
        let mut fresh = false;
        let (drag, h) = dispatch(
            &mut buf,
            &mut fresh,
            None,
            Event::Clicked((1u32, 2u32)),
            square_addr,
        );
        assert_eq!(drag, None);
        assert_eq!(h, None);
        assert_eq!(buf, "hello");
        assert!(!fresh);
    }

    #[test]
    fn dispatch_drag_started_calls_drag_start() {
        let mut buf = String::from("=SUM(");
        let mut fresh = false;
        let (drag, h) = dispatch(
            &mut buf,
            &mut fresh,
            None,
            Event::DragStarted((0u32, 0u32)),
            square_addr,
        );
        assert!(drag.is_some());
        assert!(h.is_some());
        assert_eq!(buf, "=SUM(A1");
        assert!(fresh);
    }

    #[test]
    fn dispatch_dragged_calls_drag_extend() {
        let mut buf = String::from("=SUM(A1");
        let mut fresh = false;
        let current = Some(DragState {
            start: (0u32, 0u32),
            buffer_anchor: "=SUM(".len(),
        });
        let (drag, h) = dispatch(
            &mut buf,
            &mut fresh,
            current,
            Event::Dragged((2, 3)),
            square_addr,
        );
        assert!(drag.is_some());
        assert!(h.is_some());
        assert_eq!(buf, "=SUM(A1:C4");
    }

    #[test]
    fn dispatch_clicked_calls_click_insert() {
        let mut buf = String::from("=");
        let mut fresh = false;
        let (drag, h) = dispatch(
            &mut buf,
            &mut fresh,
            None,
            Event::Clicked((1u32, 2u32)),
            square_addr,
        );
        assert_eq!(drag, None);
        assert!(h.is_some());
        assert_eq!(buf, "=B3");
    }

    #[test]
    fn dispatch_drag_stopped_clears_drag() {
        let mut buf = String::from("=SUM(A1:C4");
        let mut fresh = false;
        let current = Some(DragState {
            start: (0u32, 0u32),
            buffer_anchor: "=SUM(".len(),
        });
        let (drag, h) = dispatch(
            &mut buf,
            &mut fresh,
            current,
            Event::DragStopped,
            square_addr,
        );
        assert_eq!(drag, None, "drag_stopped clears the in-flight drag");
        assert_eq!(h, None);
        assert_eq!(buf, "=SUM(A1:C4");
    }

    #[test]
    fn dispatch_idle_returns_no_change() {
        let mut buf = String::from("=");
        let mut fresh = false;
        let (drag, h) = dispatch(&mut buf, &mut fresh, None, Event::Idle, square_addr);
        assert_eq!(drag, None);
        assert_eq!(h, None);
        assert_eq!(buf, "=");
    }

    // --- T-003: event_from_response ---

    #[test]
    fn event_from_response_clicked_returns_clicked() {
        let e: Event<(u32, u32)> = event_from_response(true, false, false, false, Some((1, 2)));
        assert_eq!(e, Event::Clicked((1, 2)));
    }

    #[test]
    fn event_from_response_drag_started_returns_drag_started() {
        let e: Event<(u32, u32)> = event_from_response(false, true, false, false, Some((0, 0)));
        assert_eq!(e, Event::DragStarted((0, 0)));
    }

    #[test]
    fn event_from_response_dragged_returns_dragged() {
        let e: Event<(u32, u32)> = event_from_response(false, false, true, false, Some((3, 4)));
        assert_eq!(e, Event::Dragged((3, 4)));
    }

    #[test]
    fn event_from_response_drag_stopped_returns_drag_stopped() {
        let e: Event<(u32, u32)> = event_from_response(false, false, false, true, None);
        assert_eq!(e, Event::DragStopped);
    }

    #[test]
    fn event_from_response_no_flags_returns_idle() {
        let e: Event<(u32, u32)> = event_from_response(false, false, false, false, Some((1, 1)));
        assert_eq!(e, Event::Idle);
    }

    #[test]
    fn event_from_response_no_cell_under_pointer_falls_back() {
        // Defensive: clicked=true with no cell → Idle, not a phantom Click.
        let e: Event<(u32, u32)> = event_from_response(true, false, false, false, None);
        assert_eq!(e, Event::Idle);
    }
}
