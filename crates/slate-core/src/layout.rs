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

    pub fn move_up(&mut self, max_rows: u16) {
        if self.row > 0 {
            self.row -= 1;
        } else {
            self.row = max_rows - 1;
        }
    }

    pub fn move_down(&mut self, max_rows: u16) {
        self.row = (self.row + 1) % max_rows;
    }

    pub fn move_left(&mut self, max_cols: u16) {
        if self.col > 0 {
            self.col -= 1;
        } else {
            self.col = max_cols - 1;
        }
    }

    pub fn move_right(&mut self, max_cols: u16) {
        self.col = (self.col + 1) % max_cols;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_movement_wraps() {
        let mut focus = FocusPosition::new(0, 0);
        focus.move_up(3);
        assert_eq!(focus.row, 2);
        focus.move_down(3);
        assert_eq!(focus.row, 0);
        focus.move_left(3);
        assert_eq!(focus.col, 2);
        focus.move_right(3);
        assert_eq!(focus.col, 0);
    }
}
