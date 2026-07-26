use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Compute grid cell rects from a total area, given rows and cols.
pub fn compute_grid(area: Rect, rows: u16, cols: u16) -> Vec<Vec<Rect>> {
    let row_constraints: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Ratio(1, rows as u32))
        .collect();
    let col_constraints: Vec<Constraint> = (0..cols)
        .map(|_| Constraint::Ratio(1, cols as u32))
        .collect();

    let row_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    row_rects
        .iter()
        .map(|row_rect| {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(col_constraints.clone())
                .split(*row_rect)
                .to_vec()
        })
        .collect()
}

/// Represents the focused widget position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusPosition {
    pub row: u16,
    pub col: u16,
}

impl FocusPosition {
    pub fn new(row: u16, col: u16) -> Self {
        Self { row, col }
    }

    pub fn move_up(&mut self, _max_rows: u16) {
        if self.row > 0 {
            self.row -= 1;
        }
    }

    pub fn move_down(&mut self, max_rows: u16) {
        if self.row < max_rows - 1 {
            self.row += 1;
        }
    }

    pub fn move_left(&mut self, _max_cols: u16) {
        if self.col > 0 {
            self.col -= 1;
        }
    }

    pub fn move_right(&mut self, max_cols: u16) {
        if self.col < max_cols - 1 {
            self.col += 1;
        }
    }

    /// Move to next widget in reading order (left-to-right, top-to-bottom).
    pub fn move_next(&mut self, max_rows: u16, max_cols: u16) {
        self.col += 1;
        if self.col >= max_cols {
            self.col = 0;
            self.row += 1;
            if self.row >= max_rows {
                self.row = 0;
            }
        }
    }

    /// Move to previous widget in reading order.
    pub fn move_prev(&mut self, max_rows: u16, max_cols: u16) {
        if self.col > 0 {
            self.col -= 1;
        } else {
            self.col = max_cols - 1;
            if self.row > 0 {
                self.row -= 1;
            } else {
                self.row = max_rows - 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_movement() {
        let mut focus = FocusPosition::new(0, 0);
        // Stays at 0 when at edge (no wrap)
        focus.move_up(3);
        assert_eq!(focus.row, 0);
        focus.move_left(3);
        assert_eq!(focus.col, 0);
        // Moves forward
        focus.move_down(3);
        assert_eq!(focus.row, 1);
        focus.move_right(3);
        assert_eq!(focus.col, 1);
        // Clamped at max
        focus = FocusPosition::new(2, 2);
        focus.move_down(3);
        assert_eq!(focus.row, 2);
        focus.move_right(3);
        assert_eq!(focus.col, 2);
    }

    #[test]
    fn test_focus_next_prev() {
        let mut focus = FocusPosition::new(0, 0);
        focus.move_next(2, 2); // (0,0) -> (0,1)
        assert_eq!((focus.row, focus.col), (0, 1));
        focus.move_next(2, 2); // (0,1) -> (1,0)
        assert_eq!((focus.row, focus.col), (1, 0));
        focus.move_next(2, 2); // (1,0) -> (1,1)
        assert_eq!((focus.row, focus.col), (1, 1));
        focus.move_next(2, 2); // (1,1) -> (0,0) wraps
        assert_eq!((focus.row, focus.col), (0, 0));

        focus = FocusPosition::new(0, 0);
        focus.move_prev(2, 2); // (0,0) -> (1,1) wraps
        assert_eq!((focus.row, focus.col), (1, 1));
        focus.move_prev(2, 2); // (1,1) -> (1,0)
        assert_eq!((focus.row, focus.col), (1, 0));
    }
}
