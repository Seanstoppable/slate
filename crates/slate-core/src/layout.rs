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

/// Compute the area for a widget that may span multiple grid cells.
/// Returns the union of grid cells from (row, col) to (row + row_span - 1, col + col_span - 1).
pub fn compute_widget_area(
    grid: &[Vec<Rect>],
    row: u16,
    col: u16,
    row_span: u16,
    col_span: u16,
) -> Option<Rect> {
    let row = row as usize;
    let col = col as usize;
    let row_span = row_span.max(1) as usize;
    let col_span = col_span.max(1) as usize;

    // Check bounds
    if row >= grid.len() || col >= grid.first().map_or(0, |r| r.len()) {
        return None;
    }

    let end_row = (row + row_span).min(grid.len());
    let end_col = (col + col_span).min(grid.first().map_or(0, |r| r.len()));

    let top_left = grid[row][col];
    let bottom_right = grid[end_row - 1][end_col - 1];

    Some(Rect::new(
        top_left.x,
        top_left.y,
        bottom_right.x + bottom_right.width - top_left.x,
        bottom_right.y + bottom_right.height - top_left.y,
    ))
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

    #[test]
    fn compute_grid_returns_expected_number_of_cells() {
        let area = Rect::new(0, 0, 120, 40);
        let grid = compute_grid(area, 2, 3);

        assert_eq!(grid.len(), 2);
        assert!(grid.iter().all(|row| row.len() == 3));
        assert_eq!(grid.iter().flatten().count(), 6);
    }

    #[test]
    fn compute_grid_handles_single_cell_grid() {
        let area = Rect::new(5, 10, 20, 8);
        let grid = compute_grid(area, 1, 1);

        assert_eq!(grid.len(), 1);
        assert_eq!(grid[0].len(), 1);
        assert_eq!(grid[0][0], area);
    }

    #[test]
    fn compute_grid_handles_three_by_three_grid() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);

        assert_eq!(grid.len(), 3);
        assert!(grid.iter().all(|row| row.len() == 3));
        assert_eq!(grid[0][0], Rect::new(0, 0, 30, 10));
        assert_eq!(grid[1][1], Rect::new(30, 10, 30, 10));
        assert_eq!(grid[2][2], Rect::new(60, 20, 30, 10));
    }

    #[test]
    fn focus_movement_updates_when_not_at_edges() {
        let mut focus = FocusPosition::new(1, 1);

        focus.move_up(3);
        focus.move_left(3);

        assert_eq!((focus.row, focus.col), (0, 0));
    }

    #[test]
    fn focus_move_prev_steps_to_previous_row_when_at_first_column() {
        let mut focus = FocusPosition::new(1, 0);

        focus.move_prev(3, 4);

        assert_eq!((focus.row, focus.col), (0, 3));
    }

    #[test]
    fn compute_widget_area_single_cell() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        // Single cell (1x1 span) matches grid cell
        let result = compute_widget_area(&grid, 1, 1, 1, 1).unwrap();
        assert_eq!(result, grid[1][1]);
    }

    #[test]
    fn compute_widget_area_col_span() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        // Widget at (0,0) spanning 2 columns
        let result = compute_widget_area(&grid, 0, 0, 1, 2).unwrap();
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 0);
        assert_eq!(result.width, 60); // 2 cols * 30
        assert_eq!(result.height, 10); // 1 row
    }

    #[test]
    fn compute_widget_area_row_span() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        // Widget at (0,0) spanning 2 rows
        let result = compute_widget_area(&grid, 0, 0, 2, 1).unwrap();
        assert_eq!(result.x, 0);
        assert_eq!(result.y, 0);
        assert_eq!(result.width, 30);
        assert_eq!(result.height, 20); // 2 rows * 10
    }

    #[test]
    fn compute_widget_area_both_spans() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        // Widget at (1,1) spanning 2x2
        let result = compute_widget_area(&grid, 1, 1, 2, 2).unwrap();
        assert_eq!(result.x, 30);
        assert_eq!(result.y, 10);
        assert_eq!(result.width, 60);
        assert_eq!(result.height, 20);
    }

    #[test]
    fn compute_widget_area_clamps_to_grid_bounds() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        // Widget at (2,2) with span 3x3 — clamped to grid edge
        let result = compute_widget_area(&grid, 2, 2, 3, 3).unwrap();
        assert_eq!(result, grid[2][2]); // Only 1 cell available
    }

    #[test]
    fn compute_widget_area_out_of_bounds_returns_none() {
        let area = Rect::new(0, 0, 90, 30);
        let grid = compute_grid(area, 3, 3);
        assert!(compute_widget_area(&grid, 5, 0, 1, 1).is_none());
        assert!(compute_widget_area(&grid, 0, 5, 1, 1).is_none());
    }
}
