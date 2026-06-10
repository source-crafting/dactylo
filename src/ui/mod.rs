pub mod config;
pub mod results;
pub mod typing;

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub fn draw_countdown(frame: &mut Frame, n: u8) {
    let area = frame.area();
    frame.render_widget(Block::bordered().title(" dactylo "), area);
    let mid = Rect {
        x: area.x,
        y: area.y + area.height / 2,
        width: area.width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(n.to_string()).centered(), mid);
}
