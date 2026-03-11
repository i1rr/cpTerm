pub mod command_bar;
pub mod dialog;
pub mod dual_pane;
pub mod explorer;
pub mod status_bar;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::action::Action;

pub trait Component {
    fn handle_action(&mut self, action: &Action) -> Option<Action>;
    fn draw(&mut self, frame: &mut Frame, area: Rect);
}
