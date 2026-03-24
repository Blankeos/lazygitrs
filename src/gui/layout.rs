use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Tracks the terminal size and computes panel rects.
#[derive(Debug)]
pub struct LayoutState {
    pub width: u16,
    pub height: u16,
    pub side_panel_ratio: f64,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
            side_panel_ratio: 0.3333,
        }
    }
}

impl LayoutState {
    pub fn update_size(&mut self, w: u16, h: u16) {
        self.width = w;
        self.height = h;
    }
}

/// The computed layout rects for a single frame.
pub struct FrameLayout {
    pub side_panels: Vec<Rect>,
    pub main_panel: Rect,
    pub status_bar: Rect,
}

pub fn compute_layout(area: Rect, side_ratio: f64, panel_count: usize) -> FrameLayout {
    // Top-level: main area + status bar at bottom
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let main_area = outer[0];
    let status_bar = outer[1];

    // Split main area into side panel and main content
    let side_width = ((main_area.width as f64) * side_ratio) as u16;
    let side_width = side_width.max(20).min(main_area.width / 2);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(side_width),
            Constraint::Min(1),
        ])
        .split(main_area);

    let side_area = horizontal[0];
    let main_panel = horizontal[1];

    // Split side area into panels
    let panel_constraints: Vec<Constraint> = (0..panel_count)
        .map(|_| Constraint::Ratio(1, panel_count as u32))
        .collect();

    let side_panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints(panel_constraints)
        .split(side_area)
        .to_vec();

    FrameLayout {
        side_panels,
        main_panel,
        status_bar,
    }
}
