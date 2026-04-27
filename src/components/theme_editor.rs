use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::theme::{
    FIELD_COUNT, FIELD_INFO, Theme, color_display_name, color_to_index, contrast_fg,
    indexed_to_color,
};

// ── Color picker ─────────────────────────────────────────

pub struct ColorPicker {
    grid: Vec<Vec<Option<u8>>>,
    pub row: usize,
    pub col: usize,
    pub original: Color,
}

impl ColorPicker {
    pub fn new(current: Color) -> Self {
        let grid = build_picker_grid();
        let idx = color_to_index(current);
        let (row, col) = find_in_grid(&grid, idx);
        Self {
            grid,
            row,
            col,
            original: current,
        }
    }

    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.clamp_col();
        }
    }

    pub fn move_down(&mut self) {
        if self.row + 1 < self.grid.len() {
            self.row += 1;
            self.clamp_col();
        }
    }

    pub fn move_left(&mut self) {
        for c in (0..self.col).rev() {
            if self.grid[self.row][c].is_some() {
                self.col = c;
                return;
            }
        }
    }

    pub fn move_right(&mut self) {
        let row_len = self.grid[self.row].len();
        for c in (self.col + 1)..row_len {
            if self.grid[self.row][c].is_some() {
                self.col = c;
                return;
            }
        }
    }

    fn clamp_col(&mut self) {
        let row = &self.grid[self.row];
        // Current position valid?
        if self.col < row.len() && row[self.col].is_some() {
            return;
        }
        // Search backward for nearest valid cell
        let max_col = self.col.min(row.len().saturating_sub(1));
        for c in (0..=max_col).rev() {
            if row[c].is_some() {
                self.col = c;
                return;
            }
        }
        // Search forward
        for (c, cell) in row.iter().enumerate() {
            if cell.is_some() {
                self.col = c;
                return;
            }
        }
    }

    pub fn current_color(&self) -> Color {
        self.grid
            .get(self.row)
            .and_then(|r| r.get(self.col))
            .and_then(|&c| c)
            .map(indexed_to_color)
            .unwrap_or(Color::Black)
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let num_rows = self.grid.len();
        let panel_height = (num_rows as u16) + 2 + 2; // grid + blank + info + borders
        let panel_width = 38_u16;
        let panel = centered_rect_fixed(panel_width, panel_height, area);
        frame.render_widget(Clear, panel);

        let color = self.current_color();
        let name = color_display_name(color);
        let idx_display = self.grid[self.row][self.col]
            .map(|i| i.to_string())
            .unwrap_or_default();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .title(Line::from(" Pick Color "))
            .title_bottom(Line::from(" Enter:Pick  Esc:Back "));

        let mut lines: Vec<Line> = Vec::new();

        for (ri, grid_row) in self.grid.iter().enumerate() {
            let mut spans = Vec::new();
            spans.push(Span::raw(" "));
            for (ci, cell) in grid_row.iter().enumerate() {
                if let Some(idx) = cell {
                    let bg = Color::Indexed(*idx);
                    if ri == self.row && ci == self.col {
                        let fg = contrast_fg(*idx);
                        spans.push(Span::styled("[]", Style::default().fg(fg).bg(bg)));
                    } else {
                        spans.push(Span::styled("  ", Style::default().bg(bg)));
                    }
                } else {
                    spans.push(Span::raw("  "));
                }
            }
            lines.push(Line::from(spans));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::raw(format!(" {} ", idx_display)),
            Span::styled("  ", Style::default().bg(color)),
            Span::raw(format!(" {}", name)),
        ]));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, panel);
    }
}

/// Build the picker grid: named colors, grayscale, then hue-sorted cube.
fn build_picker_grid() -> Vec<Vec<Option<u8>>> {
    let mut grid: Vec<Vec<Option<u8>>> = Vec::new();

    // Row 0: Named colors in rainbow order (vivid first, neutrals last)
    grid.push(
        [1, 9, 3, 11, 2, 10, 6, 14, 4, 12, 5, 13, 0, 8, 7, 15]
            .iter()
            .map(|&i| Some(i))
            .collect(),
    );

    // Rows 1-2: Grayscale (24 colors, dark to light)
    let mut gray1: Vec<Option<u8>> = (232..248).map(Some).collect();
    gray1.resize(16, None);
    grid.push(gray1);
    let mut gray2: Vec<Option<u8>> = (248..=255).map(Some).collect();
    gray2.resize(16, None);
    grid.push(gray2);

    // Rows 3+: 216-color cube sorted by hue, columns = hue, rows = dark to light
    grid.extend(build_cube_section());

    grid
}

/// Sort the 216 cube colors by hue, lay out as columns (hue) x rows (lightness).
fn build_cube_section() -> Vec<Vec<Option<u8>>> {
    struct CubeColor {
        index: u8,
        hue: f32,
        lightness: f32,
    }

    let mut colors = Vec::with_capacity(216);
    for idx in 0..216u8 {
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let (hue, lightness) = hue_lightness(r, g, b);
        colors.push(CubeColor {
            index: 16 + idx,
            hue,
            lightness,
        });
    }

    // Sort by hue (achromatic last), then dark to light
    colors.sort_by(|a, b| {
        a.hue
            .partial_cmp(&b.hue)
            .unwrap()
            .then(a.lightness.partial_cmp(&b.lightness).unwrap())
    });

    // Fill into 16 columns, top-to-bottom then left-to-right
    // Each column is a hue slice, dark at top, light at bottom
    let rows_per_col = colors.len().div_ceil(16); // ceil(216/16) = 14
    let mut grid = vec![vec![None; 16]; rows_per_col];

    for (i, color) in colors.iter().enumerate() {
        let col = i / rows_per_col;
        let row = i % rows_per_col;
        if col < 16 {
            grid[row][col] = Some(color.index);
        }
    }

    grid
}

fn hue_lightness(r: u8, g: u8, b: u8) -> (f32, f32) {
    let rf = r as f32 / 5.0;
    let gf = g as f32 / 5.0;
    let bf = b as f32 / 5.0;

    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let lightness = (max + min) / 2.0;

    if delta < 0.001 {
        // Achromatic - sort to the end
        return (999.0, lightness);
    }

    let mut hue = if (max - rf).abs() < 0.001 {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < 0.001 {
        60.0 * ((bf - rf) / delta + 2.0)
    } else {
        60.0 * ((rf - gf) / delta + 4.0)
    };

    if hue < 0.0 {
        hue += 360.0;
    }

    (hue, lightness)
}

fn find_in_grid(grid: &[Vec<Option<u8>>], target: u8) -> (usize, usize) {
    for (r, row) in grid.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if *cell == Some(target) {
                return (r, c);
            }
        }
    }
    (0, 0)
}

// ── Theme editor ─────────────────────────────────────────

#[derive(Default)]
pub struct ThemeEditor {
    pub cursor: usize,
    scroll: usize,
    pub picker: Option<ColorPicker>,
    pub naming: Option<String>,
}

impl ThemeEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < FIELD_COUNT {
            self.cursor += 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.cursor = 0;
        self.scroll = 0;
    }

    pub fn move_to_bottom(&mut self) {
        self.cursor = FIELD_COUNT - 1;
    }

    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(10);
    }

    pub fn page_down(&mut self) {
        self.cursor = (self.cursor + 10).min(FIELD_COUNT - 1);
    }

    pub fn open_picker(&mut self, current_color: Color) {
        self.picker = Some(ColorPicker::new(current_color));
    }

    pub fn start_naming(&mut self) {
        self.naming = Some(String::new());
    }

    /// Display row index for the cursor, accounting for section headers.
    fn cursor_display_row(&self) -> usize {
        let mut row = 0;
        let mut prev_section = "";
        for (i, &(section, _)) in FIELD_INFO.iter().enumerate() {
            if section != prev_section {
                if !prev_section.is_empty() {
                    row += 1;
                }
                row += 1;
                prev_section = section;
            }
            if i == self.cursor {
                return row;
            }
            row += 1;
        }
        row
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, theme_name: &str) {
        self.draw_editor(frame, area, theme, theme_name);

        if let Some(ref picker) = self.picker {
            picker.draw(frame, area, theme);
        }
    }

    fn draw_editor(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, theme_name: &str) {
        let mut lines: Vec<Line> = Vec::new();
        let mut prev_section = "";

        for (i, &(section, label)) in FIELD_INFO.iter().enumerate() {
            if section != prev_section {
                if !prev_section.is_empty() {
                    lines.push(Line::raw(""));
                }
                lines.push(Line::from(Span::styled(
                    format!(" {}", section),
                    Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )));
                prev_section = section;
            }

            let color = theme.get_field(i);
            let is_selected = i == self.cursor;
            let name = color_display_name(color);

            let swatch = Span::styled("  ", Style::default().bg(color));

            if is_selected {
                let hl = Style::default().add_modifier(Modifier::REVERSED);
                lines.push(Line::from(vec![
                    Span::styled(format!(" > {:<18}", label), hl),
                    swatch,
                    Span::styled(format!(" {:<14}", name), hl),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(format!("   {:<18}", label)),
                    swatch,
                    Span::raw(format!(" {}", name)),
                ]));
            }
        }

        // Panel sizing
        let content_height = lines.len() as u16;
        let max_height = (area.height as f32 * 0.85) as u16;
        let panel_height = (content_height + 2).min(max_height).max(10);
        let inner_height = panel_height.saturating_sub(2);

        // Scroll to keep cursor visible
        let display_row = self.cursor_display_row() as u16;
        if display_row < self.scroll as u16 {
            self.scroll = display_row as usize;
        }
        if display_row >= self.scroll as u16 + inner_height {
            self.scroll = (display_row as usize).saturating_sub(inner_height as usize) + 1;
        }

        let panel_area = centered_rect(55, panel_height, area);
        frame.render_widget(Clear, panel_area);

        let title = format!(" Theme: {} ", theme_name);

        // Bottom bar: shows naming input when active, hints otherwise
        let bottom_line = if let Some(ref name_text) = self.naming {
            Line::from(vec![
                Span::styled(" Save as: ", Style::default().fg(theme.input_fg)),
                Span::raw(name_text.as_str()),
                Span::styled("\u{2588}", Style::default().fg(theme.input_fg)),
                Span::raw("  Enter:Save  Esc:Cancel "),
            ])
        } else {
            Line::from(" Enter:Edit  Tab:Next  n:New  e:Open  F2:Save  Del:Remove  Esc:Close ")
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_focused))
            .title(Line::from(title))
            .title_bottom(bottom_line);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((self.scroll as u16, 0));
        frame.render_widget(paragraph, panel_area);
    }
}

// ── Layout helpers ───────────────────────────────────────

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
