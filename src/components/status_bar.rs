use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::InputMode;
use crate::theme::Theme;
use crate::util::format_size;

use super::dual_pane::DualPane;

pub fn draw_status_bar(
    frame: &mut Frame,
    area: Rect,
    dual_pane: &DualPane,
    input_mode: &InputMode,
    theme: &Theme,
) {
    // When the active pane is an editor, show the file path and cursor position.
    if let Some(editor) = dual_pane.active_editor() {
        let path = editor.path.to_string_lossy().to_string();
        let (row, col) = editor.cursor();
        let info = format!("{}  Ln {}, Col {}", path, row + 1, col + 1);
        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled(info, Style::default().fg(theme.path_fg)),
        ]))
        .style(Style::default().bg(theme.status_bg).fg(theme.status_fg));
        frame.render_widget(paragraph, area);
        return;
    }

    // Explorer mode
    let explorer = match dual_pane.active_explorer() {
        Some(e) => e,
        None => return,
    };
    let path = explorer.current_dir.to_string_lossy().to_string();

    let mut spans = vec![
        Span::styled(format!("{}> ", path), Style::default().fg(theme.path_fg)),
    ];

    match input_mode {
        InputMode::Command(text) => {
            spans.push(Span::styled(text.as_str(), Style::default().fg(theme.status_fg)));
            spans.push(Span::styled("_", Style::default().fg(theme.path_fg)));
        }
        _ => {
            let sel_count = explorer.selected.len();
            let total_size = explorer.selected_total_size();

            if sel_count > 0 {
                spans.push(Span::styled(
                    format!("{} selected", sel_count),
                    Style::default().fg(theme.selection_fg),
                ));
                spans.push(Span::raw(" | "));
                spans.push(Span::styled(
                    format!("{} total", format_size(total_size)),
                    Style::default().fg(theme.selection_fg),
                ));
            }

            if let Some(ref filter) = explorer.filter_text {
                if sel_count > 0 {
                    spans.push(Span::raw(" | "));
                }
                spans.push(Span::styled(
                    format!("Filter: {}", filter),
                    Style::default().fg(theme.filter_fg),
                ));
            }
        }
    }

    let paragraph = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(theme.status_bg).fg(theme.status_fg));
    frame.render_widget(paragraph, area);
}
