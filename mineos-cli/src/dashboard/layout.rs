use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Layout helper for dashboard components
pub struct DashboardLayout {
    compact: bool,
}

impl DashboardLayout {
    pub fn new(compact: bool) -> Self {
        Self { compact }
    }

    /// Get main layout chunks (header, content, footer)
    pub fn main_chunks(&self, area: Rect) -> std::rc::Rc<[Rect]> {
        let constraints = if self.compact {
            vec![
                Constraint::Length(2),   // Compact header
                Constraint::Min(8),      // Main content
                Constraint::Length(2),   // Compact footer
            ]
        } else {
            vec![
                Constraint::Length(3),   // Full header
                Constraint::Min(10),     // Main content
                Constraint::Length(3),   // Full footer
            ]
        };

        Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area)
    }

    /// Get GPU layout based on number of GPUs
    pub fn gpu_layout(&self, area: Rect, gpu_count: usize) -> Vec<Rect> {
        if gpu_count == 0 {
            return vec![area];
        }

        // Determine optimal layout based on GPU count
        let (rows, cols) = match gpu_count {
            1 => (1, 1),
            2 => (1, 2),
            3..=4 => (2, 2),
            5..=6 => (2, 3),
            7..=9 => (3, 3),
            _ => (4, 4), // Max 16 GPUs in view
        };

        // Create row chunks
        let row_constraints: Vec<Constraint> = (0..rows)
            .map(|_| Constraint::Percentage((100 / rows) as u16))
            .collect();

        let row_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(area);

        // Create column chunks for each row
        let mut gpu_chunks = Vec::new();
        let mut gpu_index = 0;

        for row_chunk in row_chunks.iter() {
            let gpus_in_row = cols.min(gpu_count - gpu_index);
            if gpus_in_row == 0 {
                break;
            }

            let col_constraints: Vec<Constraint> = (0..gpus_in_row)
                .map(|_| Constraint::Percentage((100 / gpus_in_row) as u16))
                .collect();

            let col_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(col_constraints)
                .split(*row_chunk);

            gpu_chunks.extend(col_chunks.iter().copied());
            gpu_index += gpus_in_row;
        }

        gpu_chunks
    }

    /// Get bottom panel layout (stats and logs)
    pub fn bottom_layout(&self, area: Rect) -> std::rc::Rc<[Rect]> {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40), // Stats
                Constraint::Percentage(60), // Logs
            ])
            .split(area)
    }

    /// Get help overlay dimensions
    pub fn help_overlay(&self, area: Rect) -> Rect {
        let width = if self.compact { 50 } else { 60 };
        let height = if self.compact { 30 } else { 40 };

        centered_rect(width, height, area)
    }
}

/// Helper function to create centered rectangle
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}